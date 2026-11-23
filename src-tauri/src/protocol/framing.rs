//! Framing: como separar mensagens dentro de um fluxo contínuo de bytes.
//!
//! Um stream QUIC (assim como TCP) entrega uma sequência de bytes, sem marcar
//! onde uma mensagem termina e a próxima começa. A solução mais simples é o
//! **prefixo de tamanho**: antes de cada mensagem enviamos 4 bytes dizendo
//! quantos bytes ela tem.
//!
//! ```text
//! [ tamanho: u32 big-endian ][ conteúdo: `tamanho` bytes ]
//! ```
//!
//! O limite `MAX_FRAME_LEN` protege contra um participante malicioso que diz
//! "vou mandar 4 GB" só para fazer o outro lado reservar memória.
//!
//! As funções são genéricas (`AsyncRead`/`AsyncWrite`) para funcionar com os
//! streams do iroh e também com streams em memória nos testes.

use std::fmt;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Tamanho máximo do conteúdo de um quadro: 64 KiB (limite do MVP no SPEC.md).
pub const MAX_FRAME_LEN: usize = 64 * 1024;

/// O que pode dar errado ao ler ou escrever um quadro.
#[derive(Debug)]
pub enum FrameError {
    /// O quadro (enviado ou anunciado pelo outro lado) passa do limite.
    TooLarge(usize),
    /// O outro lado fechou o stream de forma limpa, entre dois quadros.
    Closed,
    /// Qualquer outro erro de entrada/saída (conexão caiu, quadro cortado...).
    Io(std::io::Error),
}

impl fmt::Display for FrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge(len) => write!(
                f,
                "mensagem grande demais ({len} bytes; limite {MAX_FRAME_LEN})"
            ),
            Self::Closed => write!(f, "conexão encerrada pelo outro lado"),
            Self::Io(e) => write!(f, "erro de rede: {e}"),
        }
    }
}

impl std::error::Error for FrameError {}

/// Escreve `payload` precedido do seu tamanho.
pub async fn write_frame<W>(writer: &mut W, payload: &[u8]) -> Result<(), FrameError>
where
    W: AsyncWrite + Unpin,
{
    if payload.len() > MAX_FRAME_LEN {
        return Err(FrameError::TooLarge(payload.len()));
    }
    // O `as u32` é seguro: acabamos de garantir que cabe em 64 KiB.
    let len = (payload.len() as u32).to_be_bytes();
    writer.write_all(&len).await.map_err(FrameError::Io)?;
    writer.write_all(payload).await.map_err(FrameError::Io)?;
    writer.flush().await.map_err(FrameError::Io)?;
    Ok(())
}

/// Lê um quadro completo e devolve o conteúdo (sem o prefixo).
pub async fn read_frame<R>(reader: &mut R) -> Result<Vec<u8>, FrameError>
where
    R: AsyncRead + Unpin,
{
    let mut len_buf = [0u8; 4];
    match reader.read_exact(&mut len_buf).await {
        Ok(_) => {}
        // Fim do stream bem no início de um quadro = fechamento normal.
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
            return Err(FrameError::Closed);
        }
        Err(e) => return Err(FrameError::Io(e)),
    }

    let len = u32::from_be_bytes(len_buf) as usize;
    // Checar ANTES de alocar o buffer.
    if len > MAX_FRAME_LEN {
        return Err(FrameError::TooLarge(len));
    }

    let mut payload = vec![0u8; len];
    reader
        .read_exact(&mut payload)
        .await
        .map_err(FrameError::Io)?;
    Ok(payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn ida_e_volta_de_varios_quadros() {
        // `duplex` cria um "cano" em memória: o que entra de um lado sai do outro.
        let (mut a, mut b) = tokio::io::duplex(1024 * 1024);
        write_frame(&mut a, b"ola").await.unwrap();
        write_frame(&mut a, b"").await.unwrap();
        write_frame(&mut a, &vec![7u8; MAX_FRAME_LEN])
            .await
            .unwrap();

        assert_eq!(read_frame(&mut b).await.unwrap(), b"ola");
        assert_eq!(read_frame(&mut b).await.unwrap(), b"");
        assert_eq!(read_frame(&mut b).await.unwrap().len(), MAX_FRAME_LEN);
    }

    #[tokio::test]
    async fn recusa_enviar_quadro_grande_demais() {
        let (mut a, _b) = tokio::io::duplex(64);
        let err = write_frame(&mut a, &vec![0u8; MAX_FRAME_LEN + 1])
            .await
            .unwrap_err();
        assert!(matches!(err, FrameError::TooLarge(_)));
    }

    #[tokio::test]
    async fn recusa_receber_quadro_anunciado_grande_demais() {
        let (mut a, mut b) = tokio::io::duplex(64);
        // Um "atacante" anuncia 4 GB sem mandar o conteúdo.
        a.write_all(&u32::MAX.to_be_bytes()).await.unwrap();
        let err = read_frame(&mut b).await.unwrap_err();
        assert!(matches!(err, FrameError::TooLarge(_)));
    }

    #[tokio::test]
    async fn fechamento_entre_quadros_vira_closed() {
        let (a, mut b) = tokio::io::duplex(64);
        drop(a);
        assert!(matches!(
            read_frame(&mut b).await.unwrap_err(),
            FrameError::Closed
        ));
    }

    #[tokio::test]
    async fn quadro_cortado_no_meio_vira_erro_de_io() {
        let (mut a, mut b) = tokio::io::duplex(64);
        a.write_all(&10u32.to_be_bytes()).await.unwrap();
        a.write_all(b"abc").await.unwrap(); // prometeu 10, mandou 3
        drop(a);
        assert!(matches!(
            read_frame(&mut b).await.unwrap_err(),
            FrameError::Io(_)
        ));
    }
}
