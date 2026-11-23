//! Convite: o texto que o Host manda para os Convidados.
//!
//! O convite contém:
//! - o **endereço do Host** no iroh (`EndpointAddr`): a chave pública do Host
//!   e, opcionalmente, como alcançá-lo (servidor relay e/ou IPs);
//! - o **identificador da sala** (`RoomId`), 16 bytes aleatórios.
//!
//! A **senha não faz parte do convite**: ela é combinada por outro canal.
//!
//! A chave pública do Host é a parte importante para a segurança: a conexão
//! QUIC do iroh só é aceita se o outro lado provar que tem a chave privada
//! correspondente. Assim, quem tem o convite sabe que está falando com o Host
//! verdadeiro, e não com um impostor no meio do caminho.
//!
//! Formato: `sala1:` + JSON codificado em base64 (variante "URL safe", sem `=`),
//! o que gera um texto sem espaços, fácil de copiar e colar.

use std::fmt;

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use iroh::EndpointAddr;
use serde::{Deserialize, Serialize};

/// Prefixo do convite. O `1` é a versão do formato do convite.
pub const INVITE_PREFIX: &str = "sala1:";

/// Identificador aleatório da sala.
///
/// Na fase 3 ele também entra no "sal" do Argon2id e no transcript do PAKE.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RoomId(pub [u8; 16]);

impl RoomId {
    /// Gera um identificador novo usando o gerador aleatório do sistema.
    pub fn random() -> Self {
        let mut bytes = [0u8; 16];
        getrandom::fill(&mut bytes).expect("gerador aleatório do sistema indisponível");
        Self(bytes)
    }
}

/// Conteúdo de um convite.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Invite {
    pub room_id: RoomId,
    pub host: EndpointAddr,
}

/// Erro ao ler um convite. A mensagem é pensada para o usuário final.
#[derive(Debug, PartialEq, Eq)]
pub enum InviteError {
    /// Não começa com `sala1:` (texto errado, ou convite de outra versão).
    WrongPrefix,
    /// Começa certo, mas o resto está corrompido ou incompleto.
    Malformed,
}

impl fmt::Display for InviteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongPrefix => write!(
                f,
                "Isso não parece um convite do Sala (ele começa com \"{INVITE_PREFIX}\")."
            ),
            Self::Malformed => write!(
                f,
                "O convite está incompleto ou corrompido. Copie-o de novo, inteiro."
            ),
        }
    }
}

impl std::error::Error for InviteError {}

impl Invite {
    /// Transforma o convite no texto que o usuário copia.
    pub fn encode(&self) -> String {
        let json = serde_json::to_vec(self).expect("Invite sempre serializa");
        format!("{INVITE_PREFIX}{}", URL_SAFE_NO_PAD.encode(json))
    }

    /// Lê o texto colado pelo usuário. Espaços e quebras de linha nas pontas
    /// são ignorados (é comum vir assim de apps de mensagem).
    pub fn decode(text: &str) -> Result<Self, InviteError> {
        let body = text
            .trim()
            .strip_prefix(INVITE_PREFIX)
            .ok_or(InviteError::WrongPrefix)?;
        let json = URL_SAFE_NO_PAD
            .decode(body)
            .map_err(|_| InviteError::Malformed)?;
        serde_json::from_slice(&json).map_err(|_| InviteError::Malformed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iroh::SecretKey;

    fn exemplo() -> Invite {
        let host = EndpointAddr::new(SecretKey::generate().public())
            .with_ip_addr("192.168.0.10:4433".parse().unwrap())
            .with_relay_url("https://relay.example.com".parse().unwrap());
        Invite {
            room_id: RoomId::random(),
            host,
        }
    }

    #[test]
    fn ida_e_volta() {
        let invite = exemplo();
        let text = invite.encode();
        assert!(text.starts_with(INVITE_PREFIX));
        assert_eq!(Invite::decode(&text).unwrap(), invite);
    }

    #[test]
    fn ignora_espacos_nas_pontas() {
        let invite = exemplo();
        let text = format!("  \n{}\r\n ", invite.encode());
        assert_eq!(Invite::decode(&text).unwrap(), invite);
    }

    #[test]
    fn recusa_prefixo_errado() {
        assert_eq!(Invite::decode("ola mundo"), Err(InviteError::WrongPrefix));
    }

    #[test]
    fn recusa_convite_corrompido() {
        assert_eq!(Invite::decode("sala1:@@@"), Err(InviteError::Malformed));
        // base64 válido, mas não é um convite
        let lixo = format!("{INVITE_PREFIX}{}", URL_SAFE_NO_PAD.encode(b"{}"));
        assert_eq!(Invite::decode(&lixo), Err(InviteError::Malformed));
    }

    #[test]
    fn convite_cortado_e_recusado() {
        let text = exemplo().encode();
        let cortado = &text[..text.len() - 10];
        assert_eq!(Invite::decode(cortado), Err(InviteError::Malformed));
    }

    #[test]
    fn ids_de_sala_sao_diferentes() {
        assert_ne!(RoomId::random(), RoomId::random());
    }
}
