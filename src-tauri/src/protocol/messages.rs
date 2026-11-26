//! Mensagens do protocolo.
//!
//! Ordem de uma conexão:
//!
//! 1. `Hello` (os dois lados) — versão e sala;
//! 2. `PakeMsg` e `PakeConfirm` — handshake de senha (ver `net::auth`);
//! 3. `Join` (Convidado) → `Joined` + N × `Chat` com o histórico (Host);
//! 4. dali em diante: `Send` (Convidado → Host) e `Chat`,
//!    `ParticipantJoined`, `ParticipantLeft`, `Rejected`, `RoomClosed`
//!    (Host → Convidados).
//!
//! O SPEC.md lista também `Ping`/`Pong`. Não foram necessários: o QUIC do iroh
//! já manda "batimentos" (keep-alive) sozinho e detecta conexões mortas.
//! `Kick` chega na fase 6.
//!
//! O atributo `#[serde(tag = "type")]` faz cada mensagem virar um JSON com o
//! campo `type` dizendo qual é, por exemplo: `{"type":"send_message","text":"oi"}`.
//! Isso deixa o tráfego fácil de depurar.

use serde::{Deserialize, Serialize};

use crate::room::{
    invite::RoomId,
    state::{ChatMessage, Participant},
};

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

    /// Convidado → Host, logo após a senha: "quero entrar com este apelido".
    Join { nickname: String },
    /// Host → Convidado: entrou. `you` traz o apelido final (pode ter ganhado
    /// um número). Em seguida vêm `history_count` mensagens `Chat` com o
    /// histórico (uma por vez, para nenhuma passar do limite de 64 KiB).
    Joined {
        you: Participant,
        participants: Vec<Participant>,
        history_count: u32,
    },
    /// Host → todos: alguém entrou.
    ParticipantJoined { participant: Participant },
    /// Host → todos: alguém saiu.
    ParticipantLeft { participant: Participant },
    /// Convidado → Host: "publique esta mensagem". (No fio continua se chamando
    /// `send_message`, como no SPEC.md; em Rust é `Send` para não repetir
    /// "Message" do nome do enum, o que o clippy desaconselha.)
    #[serde(rename = "send_message")]
    Send { text: String },
    /// Host → todos (inclusive o autor, como confirmação): mensagem oficial,
    /// já com número e horário do Host.
    Chat { message: ChatMessage },
    /// Host → um Convidado: um pedido foi recusado (ex.: rápido demais).
    Rejected { reason: String },
    /// Host → todos: a sala foi encerrada.
    RoomClosed,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        protocol::framing::MAX_FRAME_LEN,
        room::state::{MAX_MESSAGE_CHARS, MAX_NICKNAME_CHARS},
    };

    #[test]
    fn mensagem_vira_json_legivel() {
        let json = serde_json::to_string(&Message::Send { text: "oi".into() }).unwrap();
        assert_eq!(json, r#"{"type":"send_message","text":"oi"}"#);
        let json = serde_json::to_string(&Message::RoomClosed).unwrap();
        assert_eq!(json, r#"{"type":"room_closed"}"#);
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

    /// A pior mensagem possível (texto e apelido no limite, só com caracteres
    /// que o JSON precisa "escapar") ainda cabe num quadro.
    #[test]
    fn maior_mensagem_cabe_num_quadro() {
        let msg = Message::Chat {
            message: ChatMessage {
                id: u64::MAX,
                from: u32::MAX,
                nickname: "\u{1}".repeat(MAX_NICKNAME_CHARS),
                text: "\u{1}".repeat(MAX_MESSAGE_CHARS),
                timestamp: u64::MAX,
            },
        };
        let len = serde_json::to_vec(&msg).unwrap().len();
        assert!(len <= MAX_FRAME_LEN, "{len} bytes");
    }
}
