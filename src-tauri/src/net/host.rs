//! Lado do Host: abre a sala, aceita Convidados e retransmite as mensagens.
//!
//! Fluxo de cada conexão:
//! 1. o Convidado abre um stream bidirecional e manda `Hello`;
//! 2. o Host confere versão e sala, e pede permissão ao limitador de
//!    tentativas. Se algo estiver errado, fecha a conexão sem explicar o
//!    motivo (não dar pistas a quem está tentando invadir);
//! 3. o Host responde `Hello` e os dois rodam o handshake de senha (`auth`);
//! 4. o Convidado manda `Join` com o apelido (e o token de retomada, se estiver
//!    reconectando); o Host responde `Joined`, a lista de participantes e o
//!    histórico, e avisa os outros;
//! 5. a partir daí, cada `Send` vira um `Chat` retransmitido a todos.
//!
//! # Como as mensagens chegam a todos
//!
//! Cada Convidado conectado tem uma **caixa de saída** (`outbox`): um canal com
//! espaço para `OUTBOX_CAPACITY` mensagens, esvaziado por uma tarefa que
//! escreve no stream dele. Retransmitir é só colocar a mensagem em todas as
//! caixas, sem esperar a rede. Se a caixa de alguém enche (conexão lenta ou
//! travada), essa conexão é encerrada, para não atrasar os outros.
//!
//! # Quedas
//!
//! Se a conexão de um Convidado cai sem `Leave`, ele fica **offline** (não sai
//! da sala) e pode voltar com o token de retomada. Uma tarefa de limpeza
//! remove quem fica offline por mais de `OFFLINE_GRACE`.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, Instant},
};

use anyhow::{Result, anyhow, bail};
use iroh::{
    Endpoint,
    endpoint::{Connection, RecvStream, SendStream},
};
use tokio::{
    sync::mpsc::{self, UnboundedSender},
    task::JoinHandle,
    time::timeout,
};
use zeroize::Zeroizing;

use super::{
    HANDSHAKE_TIMEOUT, NetEvent, RoomSnapshot, auth,
    endpoint::{self, NetMode},
};
use crate::{
    crypto::{
        limiter::AttemptLimiter,
        pake::{self, Role, StretchedPassword},
        password,
    },
    protocol::{PROTOCOL_VERSION, messages::Message, recv_msg, send_msg},
    room::{
        invite::{Invite, RoomId},
        state::{
            Admission, HOST_ID, ParticipantId, RoomError, RoomState, SessionId, now_millis,
        },
    },
};

/// Código de fechamento usado quando o Host recusa uma conexão.
const CLOSE_REFUSED: u32 = 1;
/// Código de fechamento quando uma conexão é substituída ou encerrada pelo Host.
const CLOSE_REPLACED: u32 = 3;
/// Mensagens que podem ficar na fila de um Convidado antes de ele ser
/// considerado lento demais.
const OUTBOX_CAPACITY: usize = 256;
/// Tempo para as últimas mensagens (ex.: `RoomClosed`) saírem antes de fechar.
const CLOSE_GRACE: Duration = Duration::from_millis(300);
/// De quanto em quanto tempo procurar quem ficou offline demais.
const EXPIRY_INTERVAL: Duration = Duration::from_secs(5);

/// Dados fixos para autenticar Convidados.
struct RoomAuth {
    room_id: RoomId,
    host_id: [u8; 32],
    /// Senha já passada pelo Argon2id (a senha original não é guardada).
    stretched: StretchedPassword,
    limiter: Mutex<AttemptLimiter>,
}

/// A conexão atual de um Convidado.
struct Outbox {
    session: SessionId,
    tx: mpsc::Sender<Message>,
    conn: Connection,
}

/// Estado vivo da sala + caixas de saída de quem está conectado.
struct Hub {
    state: RoomState,
    outboxes: HashMap<ParticipantId, Outbox>,
    /// `true` depois de `stop()`: ninguém mais entra e não geramos eventos.
    closed: bool,
}

impl Hub {
    /// Coloca `msg` na caixa de saída de todos os Convidados conectados.
    fn broadcast(&mut self, msg: &Message) {
        let mut too_slow = Vec::new();
        for (id, outbox) in &self.outboxes {
            if outbox.tx.try_send(msg.clone()).is_err() {
                too_slow.push(*id);
            }
        }
        // Fechar a conexão faz a pessoa ficar offline; o app dela reconecta
        // sozinho e recebe a foto atualizada da sala.
        for id in too_slow {
            if let Some(outbox) = self.outboxes.remove(&id) {
                outbox.conn.close(CLOSE_REPLACED.into(), b"");
            }
        }
    }

    /// Tira a caixa de saída de `id` se ela ainda for da sessão `session`.
    fn remove_outbox(&mut self, id: ParticipantId, session: SessionId) {
        if self.outboxes.get(&id).is_some_and(|o| o.session == session) {
            self.outboxes.remove(&id);
        }
    }
}

/// Tudo o que as tarefas da sala compartilham.
struct Shared {
    auth: RoomAuth,
    hub: Mutex<Hub>,
    events: UnboundedSender<NetEvent>,
}

impl Shared {
    fn hub(&self) -> MutexGuard<'_, Hub> {
        // Se outra tarefa entrou em pânico segurando a trava, seguimos mesmo
        // assim: o estado continua utilizável.
        self.hub.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn emit(&self, event: NetEvent) {
        let _ = self.events.send(event);
    }
}

/// Uma sala aberta.
pub struct Host {
    endpoint: Endpoint,
    invite: Invite,
    shared: Arc<Shared>,
    tasks: Vec<JoinHandle<()>>,
}

impl Host {
    /// Abre a sala: estica a senha, cria o endpoint, gera o convite e começa a
    /// aceitar conexões.
    pub async fn start(
        mode: NetMode,
        password: Zeroizing<String>,
        nickname: &str,
        events: UnboundedSender<NetEvent>,
    ) -> Result<Self> {
        password::validate_new(&password)?;
        let state = RoomState::new(nickname, Instant::now())?;
        let room_id = RoomId::random();

        // O Argon2id leva ~1 s de CPU: roda numa thread separada para não
        // travar as outras tarefas assíncronas.
        let stretched =
            tokio::task::spawn_blocking(move || pake::stretch_password(&password, &room_id))
                .await
                .map_err(|_| anyhow!("falha interna ao preparar a senha"))??;

        let endpoint = endpoint::bind(mode, true).await?;
        let host_addr = match endpoint::invite_addr(&endpoint, mode).await {
            Ok(addr) => addr,
            Err(e) => {
                endpoint.close().await;
                return Err(e);
            }
        };

        let invite = Invite {
            room_id,
            host: host_addr,
        };
        let shared = Arc::new(Shared {
            auth: RoomAuth {
                room_id,
                host_id: *endpoint.id().as_bytes(),
                stretched,
                limiter: Mutex::new(AttemptLimiter::default()),
            },
            hub: Mutex::new(Hub {
                state,
                outboxes: HashMap::new(),
                closed: false,
            }),
            events,
        });

        let tasks = vec![
            tokio::spawn(accept_loop(endpoint.clone(), shared.clone())),
            tokio::spawn(expiry_loop(shared.clone())),
        ];

        Ok(Self {
            endpoint,
            invite,
            shared,
            tasks,
        })
    }

    pub fn invite(&self) -> &Invite {
        &self.invite
    }

    /// Foto atual da sala, do ponto de vista do Host.
    pub fn snapshot(&self) -> RoomSnapshot {
        let hub = self.shared.hub();
        let participants = hub.state.participants();
        let me = participants
            .iter()
            .find(|p| p.id == HOST_ID)
            .cloned()
            .expect("o Host sempre está na sala");
        RoomSnapshot {
            me,
            participants,
            history: hub.state.history(),
        }
    }

    /// O Host manda uma mensagem na sala.
    pub fn send(&self, text: &str) -> Result<(), RoomError> {
        let message = {
            let mut hub = self.shared.hub();
            let message = hub
                .state
                .post(HOST_ID, text, Instant::now(), now_millis())?;
            hub.broadcast(&Message::Chat {
                message: message.clone(),
            });
            message
        };
        self.shared.emit(NetEvent::Chat { message });
        Ok(())
    }

    /// Remove alguém da sala. A pessoa recebe `Kick` e a conexão dela é
    /// encerrada; o token de retomada dela deixa de valer.
    ///
    /// Limitação: quem sabe a senha pode entrar de novo como alguém novo.
    /// Para afastar alguém de vez, encerre a sala e abra outra com outra senha.
    pub fn kick(&self, id: ParticipantId) -> Result<(), RoomError> {
        let participant = {
            let mut hub = self.shared.hub();
            let participant = hub.state.kick(id)?;
            if let Some(outbox) = hub.outboxes.remove(&id) {
                // Soltar `tx` faz a tarefa de escrita mandar o `Kick` e encerrar
                // o stream; o app do Convidado fecha a conexão ao ler.
                let _ = outbox.tx.try_send(Message::Kick { participant_id: id });
            }
            hub.broadcast(&Message::ParticipantLeft {
                participant: participant.clone(),
                kicked: true,
            });
            participant
        };
        self.shared.emit(NetEvent::ParticipantLeft {
            participant,
            kicked: true,
        });
        Ok(())
    }

    /// Só para testes: derruba a conexão de `id` como se a rede tivesse caído.
    #[cfg(test)]
    pub fn drop_connection(&self, id: ParticipantId) {
        if let Some(outbox) = self.shared.hub().outboxes.get(&id) {
            outbox.conn.close(CLOSE_REPLACED.into(), b"teste");
        }
    }

    /// Encerra a sala: avisa todos, espera um pouco e fecha as conexões.
    pub async fn stop(self) {
        {
            let mut hub = self.shared.hub();
            hub.closed = true;
            hub.broadcast(&Message::RoomClosed);
            // Soltar as caixas de saída faz cada tarefa de escrita enviar o
            // que restou e encerrar o stream.
            hub.outboxes.clear();
        }
        tokio::time::sleep(CLOSE_GRACE).await;
        // `close` avisa os Convidados e faz o `accept()` devolver `None`,
        // o que encerra o `accept_loop`.
        self.endpoint.close().await;
        for task in &self.tasks {
            task.abort();
        }
    }
}

/// Aceita conexões para sempre (até o endpoint fechar).
async fn accept_loop(endpoint: Endpoint, shared: Arc<Shared>) {
    while let Some(incoming) = endpoint.accept().await {
        let shared = shared.clone();
        // Cada Convidado é atendido numa tarefa separada, para um não travar o outro.
        tokio::spawn(async move {
            let Ok(conn) = incoming.await else {
                return; // handshake QUIC falhou: nada a fazer
            };
            serve_guest(conn, shared).await;
        });
    }
}

/// Tira da sala quem ficou offline por tempo demais.
async fn expiry_loop(shared: Arc<Shared>) {
    let mut tick = tokio::time::interval(EXPIRY_INTERVAL);
    loop {
        tick.tick().await;
        let gone = {
            let mut hub = shared.hub();
            if hub.closed {
                return;
            }
            let gone = hub.state.expire(Instant::now());
            for participant in &gone {
                hub.broadcast(&Message::ParticipantLeft {
                    participant: participant.clone(),
                    kicked: false,
                });
            }
            gone
        };
        for participant in gone {
            shared.emit(NetEvent::ParticipantLeft {
                participant,
                kicked: false,
            });
        }
    }
}

/// Motivo interno de recusa (nunca é enviado ao Convidado).
#[derive(Debug, PartialEq, Eq)]
enum Refusal {
    /// `Hello` ausente, inválido ou de outra sala.
    Protocol,
    /// Limitador de tentativas disse "agora não".
    Limited,
    /// Handshake de senha falhou (senha errada, desistência, lixo).
    Auth,
}

/// Atende um Convidado do começo ao fim.
async fn serve_guest(conn: Connection, shared: Arc<Shared>) {
    // 1-3. Hello, limitador e senha.
    let (mut send, mut recv) = match admit(&conn, &shared.auth).await {
        Ok(streams) => streams,
        Err(refusal) => {
            if refusal == Refusal::Auth {
                shared.emit(NetEvent::JoinRefused);
            }
            conn.close(CLOSE_REFUSED.into(), b"");
            return;
        }
    };

    // 4. Join: o Convidado diz o apelido (e talvez o token de retomada).
    let (nickname, resume) =
        match timeout(HANDSHAKE_TIMEOUT, recv_msg::<_, Message>(&mut recv)).await {
            Ok(Ok(Message::Join { nickname, resume })) => (nickname, resume),
            _ => {
                conn.close(CLOSE_REFUSED.into(), b"");
                return;
            }
        };

    let (outbox_tx, mut outbox_rx) = mpsc::channel(OUTBOX_CAPACITY);
    let joined = {
        let mut hub = shared.hub();
        if hub.closed {
            Err(anyhow!("A sala foi encerrada."))
        } else {
            // Retomada com token válido; senão, entrada nova.
            let admission = match resume.and_then(|t| hub.state.resume(&t)) {
                Some(a) => Ok(a),
                None => hub.state.join(&nickname, Instant::now()),
            };
            match admission {
                Ok(admission) => {
                    let me = admission.participant.clone();
                    if admission.resumed {
                        // Se a conexão antiga ainda estava pendurada, fecha.
                        if let Some(old) = hub.outboxes.remove(&me.id) {
                            old.conn.close(CLOSE_REPLACED.into(), b"");
                        }
                        hub.broadcast(&Message::ParticipantUpdated {
                            participant: me.clone(),
                        });
                    } else {
                        hub.broadcast(&Message::ParticipantJoined {
                            participant: me.clone(),
                        });
                    }
                    let participants = hub.state.participants();
                    let history = hub.state.history();
                    // A caixa entra na mesma "trava" em que tiramos a foto
                    // da sala: nada que aconteça depois se perde.
                    hub.outboxes.insert(
                        me.id,
                        Outbox {
                            session: admission.session,
                            tx: outbox_tx,
                            conn: conn.clone(),
                        },
                    );
                    Ok((admission, participants, history))
                }
                Err(e) => Err(anyhow!(e)),
            }
        }
    };
    let (admission, participants, history) = match joined {
        Ok(j) => j,
        Err(e) => {
            refuse_politely(&conn, &mut send, e.to_string()).await;
            return;
        }
    };
    let Admission {
        participant: me,
        token,
        session,
        resumed,
    } = admission;
    shared.emit(if resumed {
        NetEvent::ParticipantUpdated {
            participant: me.clone(),
        }
    } else {
        NetEvent::ParticipantJoined {
            participant: me.clone(),
        }
    });

    // Foto inicial direto no stream; o que acontecer enquanto isso espera na caixa.
    let initial = async {
        send_msg(
            &mut send,
            &Message::Joined {
                you: me.clone(),
                participants,
                history_count: history.len() as u32,
                resume_token: token,
            },
        )
        .await?;
        for message in history {
            send_msg(&mut send, &Message::Chat { message }).await?;
        }
        anyhow::Ok(())
    };

    let mut left_on_purpose = false;
    if initial.await.is_ok() {
        // Tarefa de escrita: esvazia a caixa de saída no stream.
        let writer = tokio::spawn(async move {
            while let Some(msg) = outbox_rx.recv().await {
                if send_msg(&mut send, &msg).await.is_err() {
                    break;
                }
            }
            // Caixa fechada (sala encerrada, expulsão ou lentidão): encerra o stream.
            let _ = send.finish();
        });

        // 5. Leitura: cada `Send` vira um `Chat` para todos; `Leave` é saída.
        loop {
            match recv_msg::<_, Message>(&mut recv).await {
                Ok(Message::Send { text }) => handle_send(&shared, me.id, &text),
                Ok(Message::Leave) => {
                    left_on_purpose = true;
                    break;
                }
                _ => break,
            }
        }
        // Deixa a tarefa de escrita terminar de enviar (ex.: `Kick`).
        let _ = timeout(CLOSE_GRACE, writer).await;
    }

    // Saída: de propósito → sai da sala; queda → fica offline esperando voltar.
    // Nada disso vale se esta conexão já foi substituída por outra (sessão nova).
    let event = {
        let mut hub = shared.hub();
        hub.remove_outbox(me.id, session);
        if hub.closed {
            None
        } else if left_on_purpose {
            let left = hub.state.leave(me.id, session);
            left.map(|participant| {
                hub.broadcast(&Message::ParticipantLeft {
                    participant: participant.clone(),
                    kicked: false,
                });
                NetEvent::ParticipantLeft {
                    participant,
                    kicked: false,
                }
            })
        } else {
            let offline = hub.state.disconnect(me.id, session, Instant::now());
            offline.map(|participant| {
                hub.broadcast(&Message::ParticipantUpdated {
                    participant: participant.clone(),
                });
                NetEvent::ParticipantUpdated { participant }
            })
        }
    };
    if let Some(event) = event {
        shared.emit(event);
    }
    conn.close(0u32.into(), b"");
}

/// Publica uma mensagem de um Convidado, ou avisa só a ele por que não deu.
fn handle_send(shared: &Shared, from: ParticipantId, text: &str) {
    let mut hub = shared.hub();
    let result = hub.state.post(from, text, Instant::now(), now_millis());
    match result {
        Ok(message) => {
            hub.broadcast(&Message::Chat {
                message: message.clone(),
            });
            drop(hub);
            shared.emit(NetEvent::Chat { message });
        }
        Err(e) => {
            if let Some(outbox) = hub.outboxes.get(&from) {
                let _ = outbox.tx.try_send(Message::Rejected {
                    reason: e.to_string(),
                });
            }
        }
    }
}

/// Explica a recusa ao Convidado (ex.: sala cheia) antes de fechar.
async fn refuse_politely(conn: &Connection, send: &mut SendStream, reason: String) {
    let _ = send_msg(send, &Message::Rejected { reason }).await;
    let _ = send.finish();
    // Dá um tempo para a mensagem chegar; o Convidado fecha ao lê-la.
    let _ = timeout(Duration::from_secs(2), conn.closed()).await;
    conn.close(CLOSE_REFUSED.into(), b"");
}

/// Decide se o Convidado entra. Devolve os streams prontos para uso.
async fn admit(conn: &Connection, auth: &RoomAuth) -> Result<(SendStream, RecvStream), Refusal> {
    let (mut send, mut recv) = receive_hello(conn, auth.room_id)
        .await
        .map_err(|_| Refusal::Protocol)?;

    lock_limiter(auth)
        .begin(Instant::now())
        .map_err(|_| Refusal::Limited)?;

    // A partir daqui, qualquer saída precisa avisar o limitador.
    let result = timeout(
        HANDSHAKE_TIMEOUT,
        authenticate(conn, auth, &mut send, &mut recv),
    )
    .await;
    let ok = matches!(result, Ok(Ok(())));

    let mut limiter = lock_limiter(auth);
    if ok {
        limiter.end_success();
        Ok((send, recv))
    } else {
        limiter.end_failure(Instant::now());
        Err(Refusal::Auth)
    }
}

fn lock_limiter(auth: &RoomAuth) -> MutexGuard<'_, AttemptLimiter> {
    auth.limiter.lock().unwrap_or_else(|e| e.into_inner())
}

/// Recebe e confere o `Hello` do Convidado.
async fn receive_hello(conn: &Connection, room_id: RoomId) -> Result<(SendStream, RecvStream)> {
    let (send, mut recv) = timeout(HANDSHAKE_TIMEOUT, conn.accept_bi())
        .await
        .map_err(|_| anyhow!("Convidado não abriu o stream a tempo"))?
        .map_err(|e| anyhow!("falha ao aceitar stream: {e}"))?;

    let hello: Message = timeout(HANDSHAKE_TIMEOUT, recv_msg(&mut recv))
        .await
        .map_err(|_| anyhow!("Convidado não mandou Hello a tempo"))??;

    match hello {
        Message::Hello {
            version,
            room_id: r,
        } if version == PROTOCOL_VERSION && r == room_id => Ok((send, recv)),
        _ => bail!("Hello inválido"),
    }
}

/// Responde o `Hello` e roda o handshake de senha como Host.
async fn authenticate(
    conn: &Connection,
    auth: &RoomAuth,
    send: &mut SendStream,
    recv: &mut RecvStream,
) -> Result<()> {
    send_msg(
        send,
        &Message::Hello {
            version: PROTOCOL_VERSION,
            room_id: auth.room_id,
        },
    )
    .await?;

    let guest_id = *conn.remote_id().as_bytes();
    let ctx = auth::context(conn, auth.room_id, auth.host_id, guest_id)?;
    auth::run(send, recv, &auth.stretched, &ctx, Role::Host).await
}
