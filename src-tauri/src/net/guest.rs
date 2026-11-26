//! Lado do Convidado: conecta ao Host a partir do convite.

use std::time::{Duration, Instant};

use anyhow::{Result, anyhow, bail};
use iroh::{
    Endpoint,
    endpoint::{Connection, RecvStream, SendStream},
};
use n0_future::StreamExt;
use serde::Serialize;
use tokio::{sync::mpsc::UnboundedSender, task::JoinHandle, time::timeout};
use zeroize::Zeroizing;

use super::{
    HANDSHAKE_TIMEOUT, NetEvent, PathKind, auth,
    endpoint::{self, ALPN, NetMode},
};
use crate::{
    crypto::pake::{self, Role, StretchedPassword},
    protocol::{PROTOCOL_VERSION, messages::Message, recv_msg, send_msg},
    room::invite::Invite,
};

/// Quanto esperar a conexão com o Host abrir.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
/// Quanto esperar a resposta de um ping.
const PING_TIMEOUT: Duration = Duration::from_secs(10);

/// Não deu nem para falar com o Host (offline, convite antigo, rede).
const CONNECT_FAILED: &str = "Não foi possível conectar ao Host. Confira o convite e se o Host \
     está com a sala aberta.";

/// Falamos com o Host, mas ele não deixou entrar. Genérica de propósito: não
/// diz se foi a senha, o limite de tentativas ou outra coisa.
const JOIN_FAILED: &str = "Não foi possível entrar na sala. Confira a senha e tente de novo \
     em alguns instantes.";

/// Resultado de um ping.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PingResult {
    /// Tempo de ida e volta, em milissegundos.
    pub rtt_ms: u64,
    /// Por onde o ping passou.
    pub path: PathKind,
}

/// Uma conexão ativa com o Host.
pub struct Guest {
    endpoint: Endpoint,
    conn: Connection,
    send: SendStream,
    recv: RecvStream,
    next_seq: u64,
    path_task: JoinHandle<()>,
}

impl Guest {
    /// Conecta ao Host do convite e faz o handshake (incluindo a senha).
    pub async fn connect(
        mode: NetMode,
        invite: &Invite,
        password: Zeroizing<String>,
        events: UnboundedSender<NetEvent>,
    ) -> Result<Self> {
        // Argon2id antes de conectar (~1 s), numa thread separada.
        let room_id = invite.room_id;
        let stretched =
            tokio::task::spawn_blocking(move || pake::stretch_password(&password, &room_id))
                .await
                .map_err(|_| anyhow!("falha interna ao preparar a senha"))??;

        let endpoint = endpoint::bind(mode, false).await?;
        match Self::open(&endpoint, invite, &stretched).await {
            Ok((conn, send, recv)) => {
                let path_task = tokio::spawn(watch_path(conn.clone(), events));
                Ok(Self {
                    endpoint,
                    conn,
                    send,
                    recv,
                    next_seq: 0,
                    path_task,
                })
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
    ) -> Result<(Connection, SendStream, RecvStream)> {
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

        Ok((conn, send, recv))
    }

    /// Manda um `Ping` e mede quanto tempo leva para o `Pong` voltar.
    pub async fn ping(&mut self) -> Result<PingResult> {
        self.next_seq += 1;
        let seq = self.next_seq;
        let start = Instant::now();

        send_msg(&mut self.send, &Message::Ping { seq })
            .await
            .map_err(|_| anyhow!("A conexão com o Host caiu."))?;
        let reply = timeout(PING_TIMEOUT, recv_msg::<_, Message>(&mut self.recv))
            .await
            .map_err(|_| anyhow!("O Host não respondeu a tempo."))?
            .map_err(|_| anyhow!("A conexão com o Host caiu."))?;

        match reply {
            Message::Pong { seq: s } if s == seq => Ok(PingResult {
                rtt_ms: start.elapsed().as_millis() as u64,
                path: PathKind::of(&self.conn),
            }),
            _ => bail!("Resposta inesperada do Host."),
        }
    }

    /// Caminho em uso agora (direto ou relay).
    pub fn path(&self) -> PathKind {
        PathKind::of(&self.conn)
    }

    /// Sai da sala e fecha a conexão de forma educada.
    pub async fn leave(self) {
        self.path_task.abort();
        self.conn.close(0u32.into(), b"");
        // `close` só agenda o aviso; `endpoint.close()` garante que ele seja enviado.
        self.endpoint.close().await;
    }
}

/// Avisa a interface sempre que o caminho até o Host muda, e quando a
/// conexão acaba.
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
    // O stream de caminhos termina quando a conexão fecha.
    let _ = events.send(NetEvent::Disconnected);
}
