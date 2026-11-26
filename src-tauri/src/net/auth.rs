//! Handshake de senha sobre os streams da conexão.
//!
//! Ordem das mensagens (depois da troca de `Hello`):
//!
//! ```text
//! Convidado                         Host
//!   PakeMsg(A)  ───────────────────►
//!               ◄─────────────────── PakeMsg(B)
//!   PakeConfirm(prova do Convidado) ►        Host confere; se errada, fecha.
//!               ◄─────────────────── PakeConfirm(prova do Host)
//!   Convidado confere a prova do Host.
//! ```
//!
//! O Convidado prova primeiro. Assim o Host nunca entrega uma prova sua para
//! quem ainda não demonstrou saber a senha.

use anyhow::{Result, anyhow, bail};
use iroh::endpoint::Connection;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::{
    crypto::pake::{self, EXPORTER_LABEL, PakeContext, Role},
    protocol::{messages::Message, recv_msg, send_msg},
    room::invite::RoomId,
};

/// Monta o contexto do PAKE para esta conexão.
///
/// O "channel binding" vem da sessão TLS do QUIC: os dois lados da mesma
/// conexão obtêm o mesmo valor, e qualquer outra conexão obtém outro.
pub fn context(
    conn: &Connection,
    room_id: RoomId,
    host_id: [u8; 32],
    guest_id: [u8; 32],
) -> Result<PakeContext> {
    let mut channel_binding = [0u8; 32];
    conn.export_keying_material(&mut channel_binding, EXPORTER_LABEL, &room_id.0)
        .map_err(|_| anyhow!("falha ao derivar a amarração da conexão"))?;
    Ok(PakeContext {
        room_id,
        host_id,
        guest_id,
        channel_binding,
    })
}

/// Roda o handshake de senha. `Ok(())` = os dois lados provaram saber a senha.
pub async fn run<W, R>(
    send: &mut W,
    recv: &mut R,
    stretched: &[u8; 32],
    ctx: &PakeContext,
    role: Role,
) -> Result<()>
where
    W: AsyncWrite + Unpin,
    R: AsyncRead + Unpin,
{
    let (state, outbound) = pake::start(stretched, ctx, role);
    send_msg(send, &Message::PakeMsg { bytes: outbound }).await?;

    let inbound = match recv_msg(recv).await? {
        Message::PakeMsg { bytes } => bytes,
        _ => bail!("mensagem inesperada no handshake"),
    };
    let key = state.finish(&inbound)?;

    match role {
        Role::Guest => {
            let mac = key.my_confirmation();
            send_msg(send, &Message::PakeConfirm { mac }).await?;
            let peer = expect_confirm(recv).await?;
            if !key.verify_peer(&peer) {
                bail!("o Host não confirmou a chave");
            }
        }
        Role::Host => {
            let peer = expect_confirm(recv).await?;
            if !key.verify_peer(&peer) {
                bail!("senha incorreta");
            }
            let mac = key.my_confirmation();
            send_msg(send, &Message::PakeConfirm { mac }).await?;
        }
    }
    Ok(())
}

async fn expect_confirm<R: AsyncRead + Unpin>(recv: &mut R) -> Result<Vec<u8>> {
    match recv_msg(recv).await? {
        Message::PakeConfirm { mac } => Ok(mac),
        _ => bail!("mensagem inesperada no handshake"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::pake::stretch_password;

    fn contexto() -> PakeContext {
        PakeContext {
            room_id: RoomId([7; 16]),
            host_id: [1; 32],
            guest_id: [2; 32],
            channel_binding: [3; 32],
        }
    }

    /// Roda Convidado e Host ao mesmo tempo, ligados por "canos" em memória.
    async fn rodar(pw_guest: &str, pw_host: &str) -> (Result<()>, Result<()>) {
        let ctx = contexto();
        let sg = stretch_password(pw_guest, &ctx.room_id).unwrap();
        let sh = stretch_password(pw_host, &ctx.room_id).unwrap();

        // Convidado escreve em `g_tx`, Host lê de `h_rx` (e vice-versa).
        let (mut g_tx, mut h_rx) = tokio::io::duplex(4096);
        let (mut h_tx, mut g_rx) = tokio::io::duplex(4096);

        let guest = async {
            let r = run(&mut g_tx, &mut g_rx, &sg, &ctx, Role::Guest).await;
            drop(g_tx); // fecha o "cano" para o outro lado não esperar à toa
            r
        };
        let host = async {
            let r = run(&mut h_tx, &mut h_rx, &sh, &ctx, Role::Host).await;
            drop(h_tx);
            r
        };
        tokio::join!(guest, host)
    }

    #[tokio::test]
    async fn senha_certa_passa_nos_dois_lados() {
        let (g, h) = rodar("correta-123", "correta-123").await;
        assert!(g.is_ok());
        assert!(h.is_ok());
    }

    #[tokio::test]
    async fn senha_errada_falha_nos_dois_lados() {
        let (g, h) = rodar("errada-123", "correta-123").await;
        assert!(h.is_err(), "o Host não pode aceitar");
        assert!(g.is_err(), "o Convidado não pode achar que entrou");
    }
}
