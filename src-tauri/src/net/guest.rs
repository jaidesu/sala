//! Lado do Convidado: conecta ao Host a partir do convite e participa do chat.
//!
//! Depois de entrar, duas tarefas rodam em segundo plano:
//! - `read_loop`: lê tudo o que o Host manda e transforma em `NetEvent`;
//! - `watch_path`: avisa quando o caminho muda (relay ↔ direto).
//!
//! Enviar mensagens é feito direto por `Guest::send`.

use std::time::Duration;

use anyhow::{Result, anyhow, bail};
use iroh::{
    Endpoint,
    endpoint::{Connection, RecvStream, SendStream},
};
use n0_future::StreamExt;
use tokio::{sync::mpsc::UnboundedSender, task::JoinHandle, time::timeout};
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
        state::{self, clean_nickname},
    },
};

/// Quanto esperar a conexão com o Host abrir.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

/// Não deu nem para falar com o Host (offline, convite antigo, rede).
const CONNECT_FAILED: &str = "Não foi possível conectar ao Host. Confira o convite e se o Host \
     está com a sala aberta.";

/// Falamos com o Host, mas ele não deixou entrar. Genérica de propósito: não
/// diz se foi a senha, o limite de tentativas ou outra coisa.
const JOIN_FAILED: &str = "Não foi possível entrar na sala. Confira a senha e tente de novo \
     em alguns instantes.";

/// Uma conexão ativa com o Host.
pub struct Guest {
    endpoint: Endpoint,
    conn: Connection,
    send: SendStream,
    tasks: Vec<JoinHandle<()>>,
}

impl Guest {
    /// Conecta ao Host do convite, faz o handshake (incluindo a senha) e entra
    /// na sala com `nickname`. Devolve a conexão e a foto inicial da sala.
    pub async fn connect(
        mode: NetMode,
        invite: &Invite,
        password: Zeroizing<String>,
        nickname: &str,
        events: UnboundedSender<NetEvent>,
    ) -> Result<(Self, RoomSnapshot)> {
        // Conferir o apelido antes de gastar tempo com rede e Argon2id.
        let nickname = clean_nickname(nickname)?;

        // Argon2id antes de conectar (~1 s), numa thread separada.
        let room_id = invite.room_id;
        let stretched =
            tokio::task::spawn_blocking(move || pake::stretch_password(&password, &room_id))
                .await
                .map_err(|_| anyhow!("falha interna ao preparar a senha"))??;

        let endpoint = endpoint::bind(mode, false).await?;
        match Self::open(&endpoint, invite, &stretched, nickname).await {
            Ok((conn, send, recv, snapshot)) => {
                let tasks = vec![
                    tokio::spawn(read_loop(recv, events.clone())),
                    tokio::spawn(watch_path(conn.clone(), events)),
                ];
                let guest = Self {
                    endpoint,
                    conn,
                    send,
                    tasks,
                };
                Ok((guest, snapshot))
            }
            Err(e) => {
                endpoint.close().await;
                Err(e)
            }
        }
    }

    async fn open(
        endpoint: &Endpoint,
        invite: &Invite,
        stretched: &StretchedPassword,
        nickname: String,
    ) -> Result<(Connection, SendStream, RecvStream, RoomSnapshot)> {
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

        // 6. Entrar na sala com o apelido.
        send_msg(&mut send, &Message::Join { nickname })
            .await
            .map_err(|_| anyhow!(JOIN_FAILED))?;
        let snapshot = timeout(HANDSHAKE_TIMEOUT, receive_snapshot(&mut recv))
            .await
            .map_err(|_| anyhow!(JOIN_FAILED))??;

        Ok((conn, send, recv, snapshot))
    }

    /// Envia uma mensagem. Ela só aparece na tela quando o Host a devolver
    /// (como `Chat`), já com número e horário oficiais.
    pub async fn send(&mut self, text: &str) -> Result<()> {
        state::check_text(text)?;
        send_msg(
            &mut self.send,
            &Message::SendMessage {
                text: text.to_string(),
            },
        )
        .await
        .map_err(|_| anyhow!("A conexão com o Host caiu."))
    }

    /// Caminho em uso agora (direto ou relay).
    pub fn path(&self) -> PathKind {
        PathKind::of(&self.conn)
    }

    /// Sai da sala e fecha a conexão de forma educada.
    pub async fn leave(self) {
        for task in &self.tasks {
            task.abort();
        }
        self.conn.close(0u32.into(), b"");
        // `close` só agenda o aviso; `endpoint.close()` garante que ele seja enviado.
        self.endpoint.close().await;
    }
}

/// Lê `Joined` e as mensagens do histórico que vêm logo depois.
async fn receive_snapshot(recv: &mut RecvStream) -> Result<RoomSnapshot> {
    let (me, participants, count) = match recv_msg::<_, Message>(recv).await {
        Ok(Message::Joined {
            you,
            participants,
            history_count,
        }) => (you, participants, history_count),
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
    Ok(RoomSnapshot {
        me,
        participants,
        history,
    })
}

/// Transforma o que o Host manda em eventos para a interface.
async fn read_loop(mut recv: RecvStream, events: UnboundedSender<NetEvent>) {
    loop {
        let event = match recv_msg::<_, Message>(&mut recv).await {
            Ok(Message::Chat { message }) => NetEvent::Chat { message },
            Ok(Message::ParticipantJoined { participant }) => {
                NetEvent::ParticipantJoined { participant }
            }
            Ok(Message::ParticipantLeft { participant }) => NetEvent::ParticipantLeft { participant },
            Ok(Message::Rejected { reason }) => NetEvent::Rejected { reason },
            Ok(Message::RoomClosed) => {
                let _ = events.send(NetEvent::RoomClosed);
                return;
            }
            // Mensagem que não faz sentido nesta fase (ex.: outro `Hello`): ignoramos.
            Ok(_) => continue,
            // Conexão caiu ou o Host fechou sem avisar.
            Err(_) => {
                let _ = events.send(NetEvent::Disconnected);
                return;
            }
        };
        let _ = events.send(event);
    }
}

/// Avisa a interface sempre que o caminho até o Host muda.
async fn watch_path(conn: Connection, events: UnboundedSender<NetEvent>) {
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
