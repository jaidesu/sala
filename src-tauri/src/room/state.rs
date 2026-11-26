//! Estado da sala: participantes, histórico e regras.
//!
//! Este módulo é "lógica pura": não sabe nada de rede nem de Tauri. O Host
//! chama estas funções e depois se encarrega de avisar os Convidados. Isso
//! deixa as regras fáceis de testar.
//!
//! Regras (SPEC.md, seções 5.3 e 7):
//! - apelido de 1 a 32 caracteres, sem caracteres de controle; repetido ganha
//!   um número ("Ana" → "Ana (2)");
//! - mensagem de 1 a `MAX_MESSAGE_CHARS` caracteres;
//! - cada participante tem um limite de ritmo (balde de fichas);
//! - o Host carimba `id` e `timestamp`, e guarda as últimas `HISTORY_LIMIT`.

use std::{
    collections::{BTreeMap, VecDeque},
    fmt,
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};

/// Identificador de participante, atribuído pelo Host (sequencial).
pub type ParticipantId = u32;

/// O Host é sempre o participante 0.
pub const HOST_ID: ParticipantId = 0;
pub const MAX_NICKNAME_CHARS: usize = 32;
pub const MAX_MESSAGE_CHARS: usize = 4000;
pub const HISTORY_LIMIT: usize = 1000;
/// Limite de pessoas na sala (inclui o Host). Mantém pequenas as mensagens
/// com a lista de participantes e a carga no computador do Host.
pub const MAX_PARTICIPANTS: usize = 50;

/// Balde de fichas: cada mensagem gasta 1 ficha; o balde enche com o tempo.
/// Permite rajadas curtas (até `BURST`) mas limita o ritmo médio.
const BURST: f64 = 10.0;
/// Fichas devolvidas por segundo (= ritmo médio máximo de mensagens).
const REFILL_PER_SEC: f64 = 2.0;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Participant {
    pub id: ParticipantId,
    pub nickname: String,
    pub is_host: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    /// Número sequencial dado pelo Host (define a ordem oficial).
    pub id: u64,
    pub from: ParticipantId,
    /// Apelido no momento do envio (continua aparecendo se a pessoa sair).
    pub nickname: String,
    pub text: String,
    /// Horário do Host, em milissegundos desde 1970 (UTC).
    pub timestamp: u64,
}

/// Erros das regras da sala. A mensagem é para o usuário final.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoomError {
    InvalidNickname,
    RoomFull,
    EmptyMessage,
    MessageTooLong,
    RateLimited,
    UnknownParticipant,
}

impl fmt::Display for RoomError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidNickname => write!(
                f,
                "Apelido inválido: use de 1 a {MAX_NICKNAME_CHARS} caracteres."
            ),
            Self::RoomFull => write!(f, "A sala está cheia ({MAX_PARTICIPANTS} pessoas)."),
            Self::EmptyMessage => write!(f, "A mensagem está vazia."),
            Self::MessageTooLong => write!(
                f,
                "Mensagem longa demais (máximo {MAX_MESSAGE_CHARS} caracteres)."
            ),
            Self::RateLimited => write!(f, "Muitas mensagens seguidas. Espere um pouco."),
            Self::UnknownParticipant => write!(f, "Participante desconhecido."),
        }
    }
}

impl std::error::Error for RoomError {}

#[derive(Debug)]
struct Bucket {
    tokens: f64,
    last: Instant,
}

impl Bucket {
    fn new(now: Instant) -> Self {
        Self {
            tokens: BURST,
            last: now,
        }
    }

    fn take(&mut self, now: Instant) -> bool {
        let elapsed = now.saturating_duration_since(self.last).as_secs_f64();
        self.tokens = (self.tokens + elapsed * REFILL_PER_SEC).min(BURST);
        self.last = now;
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

#[derive(Debug)]
struct Entry {
    info: Participant,
    bucket: Bucket,
}

#[derive(Debug)]
pub struct RoomState {
    participants: BTreeMap<ParticipantId, Entry>,
    history: VecDeque<ChatMessage>,
    next_participant: ParticipantId,
    next_message: u64,
}

/// Limpa e confere um apelido.
pub fn clean_nickname(raw: &str) -> Result<String, RoomError> {
    let nick = raw.trim();
    let len = nick.chars().count();
    if len == 0 || len > MAX_NICKNAME_CHARS || nick.chars().any(char::is_control) {
        return Err(RoomError::InvalidNickname);
    }
    Ok(nick.to_string())
}

/// Confere o texto de uma mensagem (também usado pelo Convidado antes de enviar).
pub fn check_text(text: &str) -> Result<(), RoomError> {
    if text.trim().is_empty() {
        return Err(RoomError::EmptyMessage);
    }
    if text.chars().count() > MAX_MESSAGE_CHARS {
        return Err(RoomError::MessageTooLong);
    }
    Ok(())
}

impl RoomState {
    /// Cria a sala com o Host como primeiro participante.
    pub fn new(host_nickname: &str, now: Instant) -> Result<Self, RoomError> {
        let nickname = clean_nickname(host_nickname)?;
        let mut participants = BTreeMap::new();
        participants.insert(
            HOST_ID,
            Entry {
                info: Participant {
                    id: HOST_ID,
                    nickname,
                    is_host: true,
                },
                bucket: Bucket::new(now),
            },
        );
        Ok(Self {
            participants,
            history: VecDeque::new(),
            next_participant: HOST_ID + 1,
            next_message: 1,
        })
    }

    /// Adiciona um Convidado. Devolve o participante com o apelido final.
    pub fn join(&mut self, nickname: &str, now: Instant) -> Result<Participant, RoomError> {
        let nickname = clean_nickname(nickname)?;
        if self.participants.len() >= MAX_PARTICIPANTS {
            return Err(RoomError::RoomFull);
        }
        let nickname = self.unique_nickname(nickname);
        let id = self.next_participant;
        self.next_participant += 1;
        let info = Participant {
            id,
            nickname,
            is_host: false,
        };
        self.participants.insert(
            id,
            Entry {
                info: info.clone(),
                bucket: Bucket::new(now),
            },
        );
        Ok(info)
    }

    /// Remove um participante. Devolve quem saiu (ou `None` se já não estava).
    pub fn leave(&mut self, id: ParticipantId) -> Option<Participant> {
        if id == HOST_ID {
            return None; // o Host não "sai": ele encerra a sala
        }
        self.participants.remove(&id).map(|e| e.info)
    }

    /// Registra uma mensagem. O Host não tem limite de ritmo.
    pub fn post(
        &mut self,
        from: ParticipantId,
        text: &str,
        now: Instant,
        timestamp: u64,
    ) -> Result<ChatMessage, RoomError> {
        check_text(text)?;
        let entry = self
            .participants
            .get_mut(&from)
            .ok_or(RoomError::UnknownParticipant)?;
        if from != HOST_ID && !entry.bucket.take(now) {
            return Err(RoomError::RateLimited);
        }
        let msg = ChatMessage {
            id: self.next_message,
            from,
            nickname: entry.info.nickname.clone(),
            text: text.to_string(),
            timestamp,
        };
        self.next_message += 1;
        self.history.push_back(msg.clone());
        while self.history.len() > HISTORY_LIMIT {
            self.history.pop_front();
        }
        Ok(msg)
    }

    pub fn participants(&self) -> Vec<Participant> {
        self.participants.values().map(|e| e.info.clone()).collect()
    }

    pub fn history(&self) -> Vec<ChatMessage> {
        self.history.iter().cloned().collect()
    }

    /// "Ana" já existe? Vira "Ana (2)", "Ana (3)"... (sem diferenciar maiúsculas).
    fn unique_nickname(&self, base: String) -> String {
        let taken = |n: &str| {
            self.participants
                .values()
                .any(|e| e.info.nickname.to_lowercase() == n.to_lowercase())
        };
        if !taken(&base) {
            return base;
        }
        for n in 2.. {
            let suffix = format!(" ({n})");
            // Corta o apelido se preciso para caber no limite com o sufixo.
            let keep = MAX_NICKNAME_CHARS - suffix.chars().count();
            let candidate: String = base.chars().take(keep).collect::<String>() + &suffix;
            if !taken(&candidate) {
                return candidate;
            }
        }
        unreachable!("sempre existe um número livre")
    }
}

/// Horário atual em milissegundos desde 1970 (para carimbar mensagens).
pub fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sala() -> (RoomState, Instant) {
        let t = Instant::now();
        (RoomState::new("Host", t).unwrap(), t)
    }

    #[test]
    fn host_e_o_participante_zero() {
        let (s, _) = sala();
        let p = s.participants();
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].id, HOST_ID);
        assert!(p[0].is_host);
    }

    #[test]
    fn apelido_e_limpo_e_validado() {
        assert_eq!(clean_nickname("  Ana  ").unwrap(), "Ana");
        assert!(clean_nickname("   ").is_err());
        assert!(clean_nickname(&"x".repeat(33)).is_err());
        assert!(clean_nickname(&"ç".repeat(32)).is_ok()); // conta caracteres, não bytes
        assert!(clean_nickname("a\u{0}b").is_err());
        assert!(clean_nickname("linha\nquebrada").is_err());
    }

    #[test]
    fn apelido_repetido_ganha_numero() {
        let (mut s, t) = sala();
        assert_eq!(s.join("Ana", t).unwrap().nickname, "Ana");
        assert_eq!(s.join("ana", t).unwrap().nickname, "ana (2)");
        assert_eq!(s.join("Ana", t).unwrap().nickname, "Ana (3)");
        assert_eq!(s.join("host", t).unwrap().nickname, "host (2)");
    }

    #[test]
    fn apelido_longo_repetido_continua_no_limite() {
        let (mut s, t) = sala();
        let longo = "x".repeat(MAX_NICKNAME_CHARS);
        s.join(&longo, t).unwrap();
        let segundo = s.join(&longo, t).unwrap().nickname;
        assert_eq!(segundo.chars().count(), MAX_NICKNAME_CHARS);
        assert!(segundo.ends_with(" (2)"));
    }

    #[test]
    fn ids_sao_sequenciais_e_saida_remove() {
        let (mut s, t) = sala();
        let a = s.join("A", t).unwrap();
        let b = s.join("B", t).unwrap();
        assert_eq!((a.id, b.id), (1, 2));
        assert_eq!(s.leave(a.id).unwrap().nickname, "A");
        assert!(s.leave(a.id).is_none());
        assert!(s.leave(HOST_ID).is_none());
        assert_eq!(s.participants().len(), 2);
    }

    #[test]
    fn sala_cheia() {
        let (mut s, t) = sala();
        for i in 1..MAX_PARTICIPANTS {
            s.join(&format!("p{i}"), t).unwrap();
        }
        assert_eq!(s.join("mais um", t), Err(RoomError::RoomFull));
    }

    #[test]
    fn mensagens_recebem_id_e_vao_para_o_historico() {
        let (mut s, t) = sala();
        let a = s.join("Ana", t).unwrap();
        let m1 = s.post(a.id, "oi", t, 1000).unwrap();
        let m2 = s.post(HOST_ID, "olá", t, 2000).unwrap();
        assert_eq!((m1.id, m2.id), (1, 2));
        assert_eq!(m1.nickname, "Ana");
        assert_eq!(s.history(), vec![m1, m2]);
    }

    #[test]
    fn mensagens_invalidas_sao_recusadas() {
        let (mut s, t) = sala();
        assert_eq!(s.post(HOST_ID, "  \n ", t, 0), Err(RoomError::EmptyMessage));
        let longa = "a".repeat(MAX_MESSAGE_CHARS + 1);
        assert_eq!(
            s.post(HOST_ID, &longa, t, 0),
            Err(RoomError::MessageTooLong)
        );
        assert_eq!(s.post(99, "oi", t, 0), Err(RoomError::UnknownParticipant));
        assert!(s.history().is_empty());
    }

    #[test]
    fn historico_guarda_so_as_ultimas() {
        let (mut s, t) = sala();
        for i in 0..(HISTORY_LIMIT + 5) {
            s.post(HOST_ID, &format!("m{i}"), t, 0).unwrap();
        }
        let h = s.history();
        assert_eq!(h.len(), HISTORY_LIMIT);
        assert_eq!(h[0].text, "m5");
    }

    #[test]
    fn limite_de_ritmo_por_participante() {
        let (mut s, t) = sala();
        let a = s.join("Ana", t).unwrap();
        for _ in 0..BURST as usize {
            s.post(a.id, "spam", t, 0).unwrap();
        }
        assert_eq!(s.post(a.id, "spam", t, 0), Err(RoomError::RateLimited));
        // Meio segundo depois, 1 ficha voltou (2 por segundo).
        let depois = t + Duration::from_millis(500);
        assert!(s.post(a.id, "ok", depois, 0).is_ok());
        assert_eq!(s.post(a.id, "spam", depois, 0), Err(RoomError::RateLimited));
        // O Host não tem limite.
        for _ in 0..50 {
            s.post(HOST_ID, "anúncio", t, 0).unwrap();
        }
    }
}
