//! Mensagens do protocolo.
//!
//! Existem as mensagens de abertura (`Hello`), do handshake de senha
//! (`PakeMsg`, `PakeConfirm`) e do teste de conexão (`Ping`, `Pong`). As
//! mensagens de chat (`ChatMessage`, `Joined`, `Kick`...) chegam na fase 4.
//!
//! O atributo `#[serde(tag = "type")]` faz cada mensagem virar um JSON com o
//! campo `type` dizendo qual é, por exemplo: `{"type":"ping","seq":3}`.
//! Isso deixa o tráfego fácil de depurar.

use serde::{Deserialize, Serialize};

use crate::room::invite::RoomId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Message {
    /// Primeira mensagem de cada lado ao abrir a conexão.
    ///
    /// O Convidado diz qual sala quer (o `room_id` vem do convite) e qual
    /// versão do protocolo fala. O Host confere as duas coisas e responde com
    /// o próprio `Hello`, ou fecha a conexão.
    Hello { version: u16, room_id: RoomId },
    /// Mensagem do SPAKE2 (33 bytes). Cada lado manda uma.
    PakeMsg { bytes: Vec<u8> },
    /// Prova (HMAC) de que o remetente chegou à mesma chave.
    PakeConfirm { mac: Vec<u8> },
    /// Convidado → Host: "você está aí?". O `seq` identifica o ping.
    Ping { seq: u64 },
    /// Host → Convidado: resposta ao `Ping` de mesmo `seq`.
    Pong { seq: u64 },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_vira_json_legivel() {
        let json = serde_json::to_string(&Message::Ping { seq: 3 }).unwrap();
        assert_eq!(json, r#"{"type":"ping","seq":3}"#);
    }

    #[test]
    fn ida_e_volta_do_hello() {
        let msg = Message::Hello {
            version: 1,
            room_id: RoomId::random(),
        };
        let json = serde_json::to_vec(&msg).unwrap();
        assert_eq!(serde_json::from_slice::<Message>(&json).unwrap(), msg);
    }

    #[test]
    fn tipo_desconhecido_e_recusado() {
        assert!(serde_json::from_str::<Message>(r#"{"type":"hack"}"#).is_err());
    }
}
