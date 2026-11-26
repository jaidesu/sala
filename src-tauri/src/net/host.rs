//! Lado do Host: abre a sala e aceita Convidados.
//!
//! Fluxo de cada conexão:
//! 1. o Convidado abre um stream bidirecional e manda `Hello`;
//! 2. o Host confere versão e sala, e pede permissão ao limitador de
//!    tentativas. Se algo estiver errado, fecha a conexão sem explicar o
//!    motivo (não dar pistas a quem está tentando invadir);
//! 3. o Host responde `Hello` e os dois rodam o handshake de senha (`auth`);
//! 4. com a senha confirmada, o Host passa a responder cada `Ping` com `Pong`.

use std::{
    sync::{Arc, Mutex, MutexGuard},
    time::Instant,
};

use anyhow::{Result, anyhow, bail};
use iroh::{
    Endpoint,
    endpoint::{Connection, RecvStream, SendStream},
};
use tokio::{sync::mpsc::UnboundedSender, task::JoinHandle, time::timeout};
use zeroize::Zeroizing;

use super::{
    HANDSHAKE_TIMEOUT, NetEvent, auth,
    endpoint::{self, NetMode},
};
use crate::{
    crypto::{
        limiter::AttemptLimiter,
        pake::{self, Role, StretchedPassword},
        password,
    },
    protocol::{PROTOCOL_VERSION, messages::Message, recv_msg, send_msg},
    room::invite::{Invite, RoomId},
};

/// Código de fechamento usado quando o Host recusa uma conexão.
const CLOSE_REFUSED: u32 = 1;

/// O que todas as conexões precisam saber para autenticar Convidados.
struct RoomAuth {
    room_id: RoomId,
    host_id: [u8; 32],
    /// Senha já passada pelo Argon2id (a senha original não é guardada).
    stretched: StretchedPassword,
    limiter: Mutex<AttemptLimiter>,
}

/// Uma sala aberta.
pub struct Host {
    endpoint: Endpoint,
    invite: Invite,
    accept_task: JoinHandle<()>,
}

impl Host {
    /// Abre a sala: estica a senha, cria o endpoint, gera o convite e começa a
    /// aceitar conexões.
    pub async fn start(
        mode: NetMode,
        password: Zeroizing<String>,
        events: UnboundedSender<NetEvent>,
    ) -> Result<Self> {
        password::validate_new(&password)?;
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
        let room = Arc::new(RoomAuth {
            room_id,
            host_id: *endpoint.id().as_bytes(),
            stretched,
            limiter: Mutex::new(AttemptLimiter::default()),
        });

        let accept_task = tokio::spawn(accept_loop(endpoint.clone(), room, events));

        Ok(Self {
            endpoint,
            invite,
            accept_task,
        })
    }

    pub fn invite(&self) -> &Invite {
        &self.invite
    }

    /// Fecha a sala e todas as conexões.
    pub async fn stop(self) {
        // `close` avisa os Convidados e faz o `accept()` devolver `None`,
        // o que encerra o `accept_loop`.
        self.endpoint.close().await;
        self.accept_task.abort();
    }
}

/// Aceita conexões para sempre (até o endpoint fechar).
async fn accept_loop(endpoint: Endpoint, room: Arc<RoomAuth>, events: UnboundedSender<NetEvent>) {
    while let Some(incoming) = endpoint.accept().await {
        let events = events.clone();
        let room = room.clone();
        // Cada Convidado é atendido numa tarefa separada, para um não travar o outro.
        tokio::spawn(async move {
            let Ok(conn) = incoming.await else {
                return; // handshake QUIC falhou: nada a fazer
            };
            serve_guest(conn, room, events).await;
        });
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
async fn serve_guest(conn: Connection, room: Arc<RoomAuth>, events: UnboundedSender<NetEvent>) {
    // Identificador curto (só para exibir). É derivado da chave pública do
    // Convidado; não é segredo.
    let guest = conn.remote_id().fmt_short().to_string();

    let (mut send, mut recv) = match admit(&conn, &room).await {
        Ok(streams) => streams,
        Err(refusal) => {
            if refusal == Refusal::Auth {
                let _ = events.send(NetEvent::JoinRefused);
            }
            conn.close(CLOSE_REFUSED.into(), b"");
            return;
        }
    };

    let _ = events.send(NetEvent::GuestConnected {
        guest: guest.clone(),
    });

    // Responde pings até o Convidado sair. Qualquer erro aqui significa que a
    // conexão acabou (saída normal, queda de rede ou mensagem inválida).
    while let Ok(Message::Ping { seq }) = recv_msg::<_, Message>(&mut recv).await {
        if send_msg(&mut send, &Message::Pong { seq }).await.is_err() {
            break;
        }
    }

    conn.close(0u32.into(), b"");
    let _ = events.send(NetEvent::GuestDisconnected { guest });
}

/// Decide se o Convidado entra. Devolve os streams prontos para uso.
async fn admit(conn: &Connection, room: &RoomAuth) -> Result<(SendStream, RecvStream), Refusal> {
    let (mut send, mut recv) = receive_hello(conn, room.room_id)
        .await
        .map_err(|_| Refusal::Protocol)?;

    lock(&room.limiter)
        .begin(Instant::now())
        .map_err(|_| Refusal::Limited)?;

    // A partir daqui, qualquer saída precisa avisar o limitador.
    let result = timeout(
        HANDSHAKE_TIMEOUT,
        authenticate(conn, room, &mut send, &mut recv),
    )
    .await;
    let ok = matches!(result, Ok(Ok(())));

    let mut limiter = lock(&room.limiter);
    if ok {
        limiter.end_success();
        Ok((send, recv))
    } else {
        limiter.end_failure(Instant::now());
        Err(Refusal::Auth)
    }
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
    room: &RoomAuth,
    send: &mut SendStream,
    recv: &mut RecvStream,
) -> Result<()> {
    send_msg(
        send,
        &Message::Hello {
            version: PROTOCOL_VERSION,
            room_id: room.room_id,
        },
    )
    .await?;

    let guest_id = *conn.remote_id().as_bytes();
    let ctx = auth::context(conn, room.room_id, room.host_id, guest_id)?;
    auth::run(send, recv, &room.stretched, &ctx, Role::Host).await
}

/// Trava o limitador. Se outra tarefa entrou em pânico segurando a trava,
/// seguimos com o valor mesmo assim (o limitador continua consistente).
fn lock(limiter: &Mutex<AttemptLimiter>) -> MutexGuard<'_, AttemptLimiter> {
    limiter.lock().unwrap_or_else(|e| e.into_inner())
}
