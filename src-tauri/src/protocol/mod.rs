//! Protocolo de mensagens entre Host e Convidados.
//!
//! - `framing`: como uma mensagem vira bytes no fio (prefixo de tamanho + limite).
//! - `messages`: quais mensagens existem.
//!
//! As funções `send_msg` e `recv_msg` juntam as duas coisas: transformam uma
//! mensagem em JSON e a enviam como um quadro, ou o contrário.

pub mod framing;
pub mod messages;

use anyhow::{Context, Result};
use serde::{Serialize, de::DeserializeOwned};
use tokio::io::{AsyncRead, AsyncWrite};

/// Versão do protocolo de rede.
///
/// Vai na mensagem `Hello` do handshake. Se Host e Convidado tiverem versões
/// incompatíveis, a conexão é recusada com uma mensagem clara, em vez de
/// falhar de forma estranha no meio da conversa.
pub const PROTOCOL_VERSION: u16 = 1;

/// Serializa `msg` em JSON e envia como um quadro.
pub async fn send_msg<W, T>(writer: &mut W, msg: &T) -> Result<()>
where
    W: AsyncWrite + Unpin,
    T: Serialize,
{
    let bytes = serde_json::to_vec(msg).context("falha ao serializar mensagem")?;
    framing::write_frame(writer, &bytes).await?;
    Ok(())
}

/// Lê um quadro e interpreta o JSON como uma mensagem do tipo `T`.
pub async fn recv_msg<R, T>(reader: &mut R) -> Result<T>
where
    R: AsyncRead + Unpin,
    T: DeserializeOwned,
{
    let bytes = framing::read_frame(reader).await?;
    // Nunca incluir o conteúdo recebido na mensagem de erro (pode ir para logs).
    let msg = serde_json::from_slice(&bytes).context("mensagem malformada")?;
    Ok(msg)
}
