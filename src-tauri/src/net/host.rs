//! Lado do Host: abre a sala e aceita Convidados.
//!
//! Fluxo de cada conexão (fase 2):
//! 1. o Convidado abre um stream bidirecional e manda `Hello`;
//! 2. o Host confere versão e sala; se algo estiver errado, fecha a conexão
//!    sem explicar o motivo (não dar pistas a quem está tentando invadir);
//! 3. o Host responde `Hello` e passa a responder cada `Ping` com `Pong`.
//!
//! Na fase 3 o handshake de senha (PAKE) entra entre os passos 1 e 3.

use anyhow::{Result, anyhow, bail};
use iroh::{Endpoint, endpoint::Connection};
use tokio::{sync::mpsc::UnboundedSender, task::JoinHandle, time::timeout};

use super::{
    HANDSHAKE_TIMEOUT, NetEvent,
    endpoint::{self, NetMode},
};
use crate::{
    protocol::{PROTOCOL_VERSION, messages::Message, recv_msg, send_msg},
    room::invite::{Invite, RoomId},
};

/// Código de fechamento usado quando o Host recusa uma conexão.
const CLOSE_REFUSED: u32 = 1;

/// Uma sala aberta.
pub struct Host {
    endpoint: Endpoint,
    invite: Invite,
    accept_task: JoinHandle<()>,
}

impl Host {
    /// Abre a sala: cria o endpoint, gera o convite e começa a aceitar conexões.
    pub async fn start(mode: NetMode, events: UnboundedSender<NetEvent>) -> Result<Self> {
        let endpoint = endpoint::bind(mode, true).await?;

        let host_addr = match endpoint::invite_addr(&endpoint, mode).await {
            Ok(addr) => addr,
            Err(e) => {
                endpoint.close().await;
                return Err(e);
            }
        };

        let invite = Invite {
            room_id: RoomId::random(),
            host: host_addr,
        };

        let accept_task = tokio::spawn(accept_loop(endpoint.clone(), invite.room_id, events));

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
async fn accept_loop(endpoint: Endpoint, room_id: RoomId, events: UnboundedSender<NetEvent>) {
    while let Some(incoming) = endpoint.accept().await {
        let events = events.clone();
        // Cada Convidado é atendido numa tarefa separada, para um não travar o outro.
        tokio::spawn(async move {
            let Ok(conn) = incoming.await else {
                return; // handshake QUIC falhou: nada a fazer
            };
            serve_guest(conn, room_id, events).await;
        });
    }
}

/// Atende um Convidado do começo ao fim.
async fn serve_guest(conn: Connection, room_id: RoomId, events: UnboundedSender<NetEvent>) {
    // Identificador curto (só para exibir). É derivado da chave pública do
    // Convidado; não é segredo.
    let guest = conn.remote_id().fmt_short().to_string();

    let Ok((mut send, mut recv)) = handshake(&conn, room_id).await else {
        conn.close(CLOSE_REFUSED.into(), b"");
        return;
    };

    let _ = events.send(NetEvent::GuestConnected {
        guest: guest.clone(),
    });

    // Responde pings até o Convidado sair. Qualquer erro aqui significa que a
    // conexão acabou (saída normal, queda de rede ou mensagem inválida).
    loop {
        match recv_msg::<_, Message>(&mut recv).await {
            Ok(Message::Ping { seq }) => {
                if send_msg(&mut send, &Message::Pong { seq }).await.is_err() {
                    break;
                }
            }
            Ok(_) | Err(_) => break,
        }
    }

    conn.close(0u32.into(), b"");
    let _ = events.send(NetEvent::GuestDisconnected { guest });
}

/// Recebe e confere o `Hello` do Convidado e responde com o do Host.
async fn handshake(
    conn: &Connection,
    room_id: RoomId,
) -> Result<(iroh::endpoint::SendStream, iroh::endpoint::RecvStream)> {
    let (mut send, mut recv) = timeout(HANDSHAKE_TIMEOUT, conn.accept_bi())
        .await
        .map_err(|_| anyhow!("Convidado não abriu o stream a tempo"))?
        .map_err(|e| anyhow!("falha ao aceitar stream: {e}"))?;

    let hello: Message = timeout(HANDSHAKE_TIMEOUT, recv_msg(&mut recv))
        .await
        .map_err(|_| anyhow!("Convidado não mandou Hello a tempo"))??;

    match hello {
        Message::Hello { version, room_id: r } if version == PROTOCOL_VERSION && r == room_id => {}
        _ => bail!("Hello inválido"),
    }

    send_msg(
        &mut send,
        &Message::Hello {
            version: PROTOCOL_VERSION,
            room_id,
        },
    )
    .await?;

    Ok((send, recv))
}
