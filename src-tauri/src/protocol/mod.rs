//! Protocolo de mensagens entre Host e Convidados.
//!
//! Na fase 4 este módulo ganha `messages.rs` (os tipos de mensagem) e
//! `framing.rs` (prefixo de tamanho e limites). Por enquanto só guarda a
//! versão do protocolo, que o SPEC.md pede para existir desde o início.

/// Versão do protocolo de rede.
///
/// Vai na mensagem `Hello` do handshake. Se Host e Convidado tiverem versões
/// incompatíveis, a conexão é recusada com uma mensagem clara, em vez de
/// falhar de forma estranha no meio da conversa.
pub const PROTOCOL_VERSION: u16 = 1;
