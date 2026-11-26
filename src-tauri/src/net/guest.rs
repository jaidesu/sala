//! Lado do Convidado: conecta ao Host, participa do chat e reconecta sozinho.
//!
//! # Como funciona
//!
//! Depois de entrar, uma tarefa em segundo plano (`run`) cuida de tudo:
//!
//! - repassa o que o Host manda para a interface (`NetEvent`);
//! - envia o que a interface pede (comandos `Cmd` por um canal);
//! - se a conexão cai, avisa "reconectando" e tenta de novo com espera
//!   crescente (1 s, 2 s, 4 s... até 15 s), por até `RECONNECT_WINDOW`.
//!   Ao reconectar, apresenta o **token de retomada** e volta como a mesma
//!   pessoa, recebendo a foto atualizada da sala.
//!
//! A leitura do stream fica numa tarefa própria (`reader`) que entrega as
//! mensagens por um canal. Isso é necessário porque ler um quadro não pode ser
//! interrompido no meio (o `select!` cancelaria a leitura e perderia bytes),
//! mas esperar num canal pode.

use std::time::{Duration, Instant};

use anyhow::{Result, anyhow, bail};
use iroh::{
    Endpoint,
    endpoint::{Connection, RecvStream, SendStream},
};
use n0_future::StreamExt;
use tokio::{
    sync::{mpsc, oneshot},
    task::JoinHandle,
    time::timeout,
};
use zeroize::Zeroizing;

use super::{
    HANDSHAKE_TIMEOUT, NetEvent, PathKind, RoomSnapshot, auth,
    endpoint::{self, ALPN, NetMode},
};
use crate::{
    crypto::pake::{self, Role, StretchedPassword},
    protocol::{PROTOCOL_VERSION, messages::Message, recv_msg, send_msg},
    room::{
        invite::Invite,
        state::{self, ParticipantId, ResumeToken, clean_nickname},
    },
};

/// Quanto esperar a conexão com o Host abrir.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
/// Por quanto tempo tentar reconectar antes de desistir.
const RECONNECT_WINDOW: Duration = Duration::from_secs(120);
/// Espera entre tentativas de reconexão: começa assim e dobra até `MAX`.
const RECONNECT_FIRST_DELAY: Duration = Duration::from_secs(1);
const RECONNECT_MAX_DELAY: Duration = Duration::from_secs(15);
/// Quanto esperar a saída educada (`Leave`) antes de simplesmente fechar.
const LEAVE_TIMEOUT: Duration = Duration::from_secs(3);

/// Não deu nem para falar com o Host (offline, convite antigo, rede).
const CONNECT_FAILED: &str = "Não foi possível conectar ao Host. Confira o convite e se o Host \
     está com a sala aberta.";

/// Falamos com o Host, mas ele não deixou entrar. Genérica de propósito: não
/// diz se foi a senha, o limite de tentativas ou outra coisa.
const JOIN_FAILED: &str = "Não foi possível entrar na sala. Confira a senha e tente de novo \
     em alguns instantes.";

const NOT_IN_ROOM: &str = "Você não está mais na sala.";
const RECONNECTING: &str = "Reconectando ao Host… tente de novo em instantes.";

/// Pedidos da interface para a tarefa da conexão.
enum Cmd {
    Send {
        text: String,
        reply: oneshot::Sender<Result<(), String>>,
    },
    Leave {
        done: oneshot::Sender<()>,
    },
}

/// Uma participação ativa numa sala.
pub struct Guest {
    cmds: mpsc::Sender<Cmd>,
    task: JoinHandle<()>,
}

/// O que é preciso para (re)conectar.
struct Ctx {
    endpoint: Endpoint,
    invite: Invite,
    stretched: StretchedPassword,
    nickname: String,
    token: ResumeToken,
    /// Meu id na sala (para reconhecer um `Kick` dirigido a mim).
    me_id: ParticipantId,
    events: mpsc::UnboundedSender<NetEvent>,
}

impl Ctx {
    fn emit(&self, event: NetEvent) {
        let _ = self.events.send(event);
    }
}

/// Uma conexão aberta (muda a cada reconexão).
struct Session {
    conn: Connection,
    send: SendStream,
    inbox: mpsc::Receiver<Message>,
    tasks: [JoinHandle<()>; 2],
}

impl Session {
    fn start(
        conn: Connection,
        send: SendStream,
        recv: RecvStream,
        events: mpsc::UnboundedSender<NetEvent>,
    ) -> Self {
        let (tx, inbox) = mpsc::channel(256);
        let tasks = [
            tokio::spawn(reader(recv, tx)),
            tokio::spawn(watch_path(conn.clone(), events)),
        ];
        Self {
            conn,
            send,
            inbox,
            tasks,
        }
    }

    fn close(&self) {
        for task in &self.tasks {
            task.abort();
        }
        self.conn.close(0u32.into(), b"");
    }
}

/// Resultado de uma conexão completa (handshake + entrada na sala).
struct Opened {
    conn: Connection,
    send: SendStream,
    recv: RecvStream,
    room: RoomSnapshot,
    token: ResumeToken,
}

impl Guest {
    /// Conecta ao Host do convite, faz o handshake (incluindo a senha) e entra
    /// na sala com `nickname`. Devolve a participação, a foto inicial da sala e
    /// o caminho em uso (direto ou relay).
    pub async fn connect(
        mode: NetMode,
        invite: &Invite,
        password: Zeroizing<String>,
        nickname: &str,
        events: mpsc::UnboundedSender<NetEvent>,
    ) -> Result<(Self, RoomSnapshot, PathKind)> {
        // Conferir o apelido antes de gastar tempo com rede e Argon2id.
        let nickname = clean_nickname(nickname)?;

        // Argon2id antes de conectar (~1 s), numa thread separada. O resultado
        // fica guardado (e é apagado da memória no fim) para reconectar sem
        // pedir a senha de novo.
        let room_id = invite.room_id;
        let stretched =
            tokio::task::spawn_blocking(move || pake::stretch_password(&password, &room_id))
                .await
                .map_err(|_| anyhow!("falha interna ao preparar a senha"))??;

        let endpoint = endpoint::bind(mode, false).await?;
        let opened = match open(&endpoint, invite, &stretched, &nickname, None).await {
            Ok(o) => o,
            Err(e) => {
                endpoint.close().await;
                return Err(e);
            }
        };

        let path = PathKind::of(&opened.conn);
        let room = opened.room.clone();
        let ctx = Ctx {
            endpoint,
            invite: invite.clone(),
            stretched,
            nickname,
            token: opened.token,
            me_id: room.me.id,
            events: events.clone(),
        };
        let session = Session::start(opened.conn, opened.send, opened.recv, events);
        let (cmds, cmd_rx) = mpsc::channel(32);
        let task = tokio::spawn(run(ctx, session, cmd_rx));
        Ok((Self { cmds, task }, room, path))
    }

    /// Envia uma mensagem. Ela só aparece na tela quando o Host a devolver
    /// (como `Chat`), já com número e horário oficiais.
    pub async fn send(&self, text: &str) -> Result<()> {
        state::check_text(text)?;
        let (reply, answer) = oneshot::channel();
        self.cmds
            .send(Cmd::Send {
                text: text.to_string(),
                reply,
            })
            .await
            .map_err(|_| anyhow!(NOT_IN_ROOM))?;
        answer
            .await
            .map_err(|_| anyhow!(NOT_IN_ROOM))?
            .map_err(|e| anyhow!(e))
    }

    /// Sai da sala de propósito (o Host remove na hora, sem esperar reconexão).
    pub async fn leave(self) {
        let (done, finished) = oneshot::channel();
        if self.cmds.send(Cmd::Leave { done }).await.is_ok() {
            let _ = timeout(LEAVE_TIMEOUT, finished).await;
        }
        self.task.abort();
    }
}

/// Faz a conexão completa: QUIC, `Hello`, senha e `Join`.
async fn open(
    endpoint: &Endpoint,
    invite: &Invite,
    stretched: &StretchedPassword,
    nickname: &str,
    resume: Option<ResumeToken>,
) -> Result<Opened> {
    // 1. Conexão QUIC. O iroh tenta conexão direta e, se não der, usa o relay.
    //    A conexão só abre se o Host provar que é dono da chave do convite.
    let conn = timeout(CONNECT_TIMEOUT, endpoint.connect(invite.host.clone(), ALPN))
        .await
        .map_err(|_| anyhow!(CONNECT_FAILED))?
        .map_err(|_| anyhow!(CONNECT_FAILED))?;

    // 2. Stream bidirecional onde vão todas as mensagens.
    let (mut send, mut recv) = conn.open_bi().await.map_err(|_| anyhow!(CONNECT_FAILED))?;

    // 3. Hello: dizemos qual sala queremos e qual versão falamos.
    send_msg(
        &mut send,
        &Message::Hello {
            version: PROTOCOL_VERSION,
            room_id: invite.room_id,
        },
    )
    .await
    .map_err(|_| anyhow!(CONNECT_FAILED))?;

    // 4. O Host responde Hello se aceitou; se recusou, fecha a conexão.
    let reply = timeout(HANDSHAKE_TIMEOUT, recv_msg::<_, Message>(&mut recv))
        .await
        .map_err(|_| anyhow!(JOIN_FAILED))?
        .map_err(|_| anyhow!(JOIN_FAILED))?;
    match reply {
        Message::Hello { version, .. } if version == PROTOCOL_VERSION => {}
        Message::Hello { .. } => bail!("O Host usa outra versão do Sala. Atualizem o app."),
        _ => bail!(JOIN_FAILED),
    }

    // 5. Handshake de senha. `remote_id` é a chave do Host já verificada
    //    pelo TLS; `endpoint.id()` é a nossa.
    let host_id = *conn.remote_id().as_bytes();
    let guest_id = *endpoint.id().as_bytes();
    let ctx = auth::context(&conn, invite.room_id, host_id, guest_id)
        .map_err(|_| anyhow!(JOIN_FAILED))?;
    timeout(
        HANDSHAKE_TIMEOUT,
        auth::run(&mut send, &mut recv, stretched, &ctx, Role::Guest),
    )
    .await
    .map_err(|_| anyhow!(JOIN_FAILED))?
    .map_err(|_| anyhow!(JOIN_FAILED))?;

    // 6. Entrar na sala com o apelido (ou retomar o lugar com o token).
    send_msg(
        &mut send,
        &Message::Join {
            nickname: nickname.to_string(),
            resume,
        },
    )
    .await
    .map_err(|_| anyhow!(JOIN_FAILED))?;
    let (room, token) = timeout(HANDSHAKE_TIMEOUT, receive_snapshot(&mut recv))
        .await
        .map_err(|_| anyhow!(JOIN_FAILED))??;

    Ok(Opened {
        conn,
        send,
        recv,
        room,
        token,
    })
}

/// Lê `Joined` e as mensagens do histórico que vêm logo depois.
async fn receive_snapshot(recv: &mut RecvStream) -> Result<(RoomSnapshot, ResumeToken)> {
    let (me, participants, count, token) = match recv_msg::<_, Message>(recv).await {
        Ok(Message::Joined {
            you,
            participants,
            history_count,
            resume_token,
        }) => (you, participants, history_count, resume_token),
        // Recusa com motivo (ex.: sala cheia, apelido inválido).
        Ok(Message::Rejected { reason }) => bail!(reason),
        _ => bail!(JOIN_FAILED),
    };

    let mut history = Vec::with_capacity(count.min(state::HISTORY_LIMIT as u32) as usize);
    for _ in 0..count {
        match recv_msg::<_, Message>(recv).await {
            Ok(Message::Chat { message }) => history.push(message),
            _ => bail!(JOIN_FAILED),
        }
    }
    let room = RoomSnapshot {
        me,
        participants,
        history,
    };
    Ok((room, token))
}

/// Lê o stream e entrega cada mensagem no canal. Termina (fechando o canal)
/// quando a conexão acaba.
async fn reader(mut recv: RecvStream, tx: mpsc::Sender<Message>) {
    while let Ok(msg) = recv_msg::<_, Message>(&mut recv).await {
        if tx.send(msg).await.is_err() {
            return;
        }
    }
}

/// Como termina uma fase "conectado".
enum Ended {
    /// A conexão caiu: tentar reconectar.
    Lost,
    /// Acabou de vez (saída, expulsão, sala encerrada).
    Done,
}

/// Resultado das tentativas de reconexão.
enum Reconnected {
    Yes(Box<Opened>),
    /// O usuário pediu para sair enquanto reconectava.
    Leave(Option<oneshot::Sender<()>>),
    /// Passou `RECONNECT_WINDOW` sem conseguir.
    GaveUp,
}

/// A vida da participação: conectado → (queda → reconectando → conectado)*.
async fn run(mut ctx: Ctx, mut session: Session, mut cmds: mpsc::Receiver<Cmd>) {
    loop {
        if let Ended::Done = connected(&ctx, &mut session, &mut cmds).await {
            break;
        }
        session.close();
        ctx.emit(NetEvent::Reconnecting);
        match reconnect(&ctx, &mut cmds).await {
            Reconnected::Yes(opened) => {
                let Opened {
                    conn,
                    send,
                    recv,
                    room,
                    token,
                } = *opened;
                let path = PathKind::of(&conn);
                ctx.token = token;
                ctx.me_id = room.me.id;
                ctx.emit(NetEvent::Resynced { room, path });
                session = Session::start(conn, send, recv, ctx.events.clone());
            }
            Reconnected::Leave(done) => {
                ctx.endpoint.close().await;
                if let Some(done) = done {
                    let _ = done.send(());
                }
                return;
            }
            Reconnected::GaveUp => {
                ctx.emit(NetEvent::Disconnected);
                ctx.endpoint.close().await;
                return;
            }
        }
    }
    session.close();
    ctx.endpoint.close().await;
}

/// Fase "conectado": repassa mensagens e atende comandos até a conexão cair
/// (`Lost`) ou a participação acabar (`Done`).
async fn connected(ctx: &Ctx, session: &mut Session, cmds: &mut mpsc::Receiver<Cmd>) -> Ended {
    loop {
        tokio::select! {
            cmd = cmds.recv() => match cmd {
                Some(Cmd::Send { text, reply }) => {
                    let result = send_msg(&mut session.send, &Message::Send { text })
                        .await
                        .map_err(|_| RECONNECTING.to_string());
                    let _ = reply.send(result);
                }
                Some(Cmd::Leave { done }) => {
                    // Avisa o Host que é uma saída de propósito e espera ele
                    // fechar a conexão (ou um tempo curto).
                    let _ = send_msg(&mut session.send, &Message::Leave).await;
                    let _ = session.send.finish();
                    let _ = timeout(Duration::from_millis(500), session.conn.closed()).await;
                    session.close();
                    ctx.endpoint.close().await;
                    let _ = done.send(());
                    return Ended::Done;
                }
                None => return Ended::Done,
            },
            msg = session.inbox.recv() => match msg {
                Some(Message::RoomClosed) => {
                    ctx.emit(NetEvent::RoomClosed);
                    return Ended::Done;
                }
                Some(Message::Kick { participant_id }) if participant_id == ctx.me_id => {
                    ctx.emit(NetEvent::Kicked);
                    return Ended::Done;
                }
                Some(other) => {
                    if let Some(event) = to_event(other) {
                        ctx.emit(event);
                    }
                }
                // Canal fechado = a leitura parou = conexão caiu.
                None => return Ended::Lost,
            },
        }
    }
}

/// Tenta reconectar com espera crescente, atendendo comandos no meio tempo.
async fn reconnect(ctx: &Ctx, cmds: &mut mpsc::Receiver<Cmd>) -> Reconnected {
    let deadline = Instant::now() + RECONNECT_WINDOW;
    let mut delay = RECONNECT_FIRST_DELAY;
    loop {
        if Instant::now() >= deadline {
            return Reconnected::GaveUp;
        }

        // Espera antes de tentar.
        let sleep = tokio::time::sleep(delay);
        tokio::pin!(sleep);
        loop {
            tokio::select! {
                _ = &mut sleep => break,
                cmd = cmds.recv() => {
                    if let Some(r) = answer_while_offline(cmd) {
                        return r;
                    }
                }
            }
        }

        // Tenta (com o token, para voltar como a mesma pessoa).
        let attempt = open(
            &ctx.endpoint,
            &ctx.invite,
            &ctx.stretched,
            &ctx.nickname,
            Some(ctx.token),
        );
        tokio::pin!(attempt);
        let result = loop {
            tokio::select! {
                r = &mut attempt => break r,
                cmd = cmds.recv() => {
                    if let Some(r) = answer_while_offline(cmd) {
                        return r;
                    }
                }
            }
        };
        match result {
            Ok(opened) => return Reconnected::Yes(Box::new(opened)),
            Err(_) => delay = (delay * 2).min(RECONNECT_MAX_DELAY),
        }
    }
}

/// Enquanto reconecta: envios são recusados com aviso; sair encerra tudo.
fn answer_while_offline(cmd: Option<Cmd>) -> Option<Reconnected> {
    match cmd {
        Some(Cmd::Send { reply, .. }) => {
            let _ = reply.send(Err(RECONNECTING.to_string()));
            None
        }
        Some(Cmd::Leave { done }) => Some(Reconnected::Leave(Some(done))),
        None => Some(Reconnected::Leave(None)),
    }
}

/// Converte uma mensagem do Host no evento correspondente para a interface.
fn to_event(msg: Message) -> Option<NetEvent> {
    Some(match msg {
        Message::Chat { message } => NetEvent::Chat { message },
        Message::ParticipantJoined { participant } => NetEvent::ParticipantJoined { participant },
        Message::ParticipantUpdated { participant } => {
            NetEvent::ParticipantUpdated { participant }
        }
        Message::ParticipantLeft {
            participant,
            kicked,
        } => NetEvent::ParticipantLeft {
            participant,
            kicked,
        },
        Message::Rejected { reason } => NetEvent::Rejected { reason },
        // O resto não faz sentido nesta fase da conexão: ignoramos.
        _ => return None,
    })
}

/// Avisa a interface sempre que o caminho até o Host muda.
async fn watch_path(conn: Connection, events: mpsc::UnboundedSender<NetEvent>) {
    let mut last = None;
    let mut paths = conn.paths_stream();
    while let Some(list) = paths.next().await {
        let kind = PathKind::classify(
            list.iter()
                .map(|p| (p.is_selected(), p.is_ip(), p.is_relay())),
        );
        if last != Some(kind) {
            last = Some(kind);
            let _ = events.send(NetEvent::PathChanged { path: kind });
        }
    }
}
