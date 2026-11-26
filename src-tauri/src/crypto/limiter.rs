//! Limite de tentativas de senha (proteção contra força bruta).
//!
//! O SPAKE2 garante que cada palpite de senha exige uma conversa com o Host.
//! Este módulo garante que o Host aceite **poucas** dessas conversas:
//!
//! - As primeiras `FREE_FAILURES` falhas não têm penalidade (todo mundo erra).
//! - Depois disso, cada falha bloqueia novas tentativas por um tempo que dobra
//!   a cada vez (2 s, 4 s, 8 s... até 5 min): o "backoff exponencial".
//! - No máximo `MAX_IN_PROGRESS` handshakes ao mesmo tempo, para um atacante
//!   não disparar centenas de palpites em paralelo antes da primeira falha.
//! - Sem falhas por `RESET_AFTER`, a contagem zera.
//!
//! O limite é **por sala**, e não por Convidado: um atacante pode gerar
//! identidades novas à vontade, e o IP nem sempre é visível (relay).
//! Efeito colateral conhecido: um atacante consegue atrasar a entrada de
//! convidados legítimos enquanto ataca. Preferimos isso a deixar a senha
//! ser adivinhada.
//!
//! O relógio (`now`) é sempre recebido como parâmetro, o que permite testar
//! "o tempo passando" sem esperar de verdade.

use std::time::{Duration, Instant};

/// Falhas permitidas antes de começar a bloquear.
pub const FREE_FAILURES: u32 = 5;
/// Primeiro bloqueio; depois dobra a cada falha.
const BASE_DELAY: Duration = Duration::from_secs(2);
/// Bloqueio máximo.
const MAX_DELAY: Duration = Duration::from_secs(5 * 60);
/// Tempo sem falhas para esquecer as anteriores.
const RESET_AFTER: Duration = Duration::from_secs(15 * 60);
/// Handshakes de senha simultâneos permitidos.
pub const MAX_IN_PROGRESS: u32 = 3;

/// Por que uma tentativa foi recusada antes mesmo de começar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refused {
    /// Bloqueado por falhas recentes; libera depois de `Duration`.
    /// (O Host não conta isso ao Convidado; o valor serve para testes e depuração.)
    #[allow(dead_code)]
    Blocked(Duration),
    /// Handshakes demais acontecendo ao mesmo tempo.
    Busy,
}

#[derive(Debug, Default)]
pub struct AttemptLimiter {
    failures: u32,
    last_failure: Option<Instant>,
    blocked_until: Option<Instant>,
    in_progress: u32,
}

impl AttemptLimiter {
    /// Pede permissão para começar um handshake de senha.
    /// Se der `Ok`, é obrigatório chamar `end_success` ou `end_failure` depois.
    pub fn begin(&mut self, now: Instant) -> Result<(), Refused> {
        self.forget_old_failures(now);
        if let Some(until) = self.blocked_until
            && now < until
        {
            return Err(Refused::Blocked(until - now));
        }
        if self.in_progress >= MAX_IN_PROGRESS {
            return Err(Refused::Busy);
        }
        self.in_progress += 1;
        Ok(())
    }

    /// O handshake terminou com a senha certa.
    ///
    /// Repare que acertar **não** zera as falhas: se zerasse, um convidado
    /// legítimo entrando no meio de um ataque daria um novo lote de
    /// tentativas livres ao atacante.
    pub fn end_success(&mut self) {
        self.in_progress = self.in_progress.saturating_sub(1);
    }

    /// O handshake falhou (senha errada, desistência ou mensagem inválida).
    pub fn end_failure(&mut self, now: Instant) {
        self.in_progress = self.in_progress.saturating_sub(1);
        self.forget_old_failures(now);
        self.failures += 1;
        self.last_failure = Some(now);

        if self.failures > FREE_FAILURES {
            // 2^0, 2^1, 2^2... vezes o atraso base, limitado a MAX_DELAY.
            let exponent = (self.failures - FREE_FAILURES - 1).min(16);
            let delay = BASE_DELAY.saturating_mul(1 << exponent).min(MAX_DELAY);
            self.blocked_until = Some(now + delay);
        }
    }

    fn forget_old_failures(&mut self, now: Instant) {
        if let Some(last) = self.last_failure
            && now.duration_since(last) >= RESET_AFTER
        {
            self.failures = 0;
            self.last_failure = None;
            self.blocked_until = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn falhar(lim: &mut AttemptLimiter, now: Instant) {
        lim.begin(now).expect("deveria permitir");
        lim.end_failure(now);
    }

    #[test]
    fn primeiras_falhas_sao_livres() {
        let mut lim = AttemptLimiter::default();
        let t0 = Instant::now();
        for _ in 0..FREE_FAILURES {
            falhar(&mut lim, t0);
        }
        assert!(lim.begin(t0).is_ok());
    }

    #[test]
    fn bloqueio_cresce_exponencialmente() {
        let mut lim = AttemptLimiter::default();
        let t0 = Instant::now();
        for _ in 0..FREE_FAILURES {
            falhar(&mut lim, t0);
        }
        // 6ª falha: bloqueia 2 s
        falhar(&mut lim, t0);
        assert_eq!(lim.begin(t0), Err(Refused::Blocked(Duration::from_secs(2))));

        // depois de 2 s libera; 7ª falha bloqueia 4 s
        let t1 = t0 + Duration::from_secs(2);
        falhar(&mut lim, t1);
        assert_eq!(lim.begin(t1), Err(Refused::Blocked(Duration::from_secs(4))));
        assert!(lim.begin(t1 + Duration::from_secs(4)).is_ok());
    }

    #[test]
    fn bloqueio_tem_teto() {
        let mut lim = AttemptLimiter::default();
        let mut t = Instant::now();
        for _ in 0..40 {
            falhar(&mut lim, t);
            t += MAX_DELAY; // espera o bloqueio máximo a cada vez
        }
        let t_final = t - MAX_DELAY;
        assert_eq!(lim.begin(t_final), Err(Refused::Blocked(MAX_DELAY)));
    }

    #[test]
    fn falhas_antigas_sao_esquecidas() {
        let mut lim = AttemptLimiter::default();
        let t0 = Instant::now();
        for _ in 0..(FREE_FAILURES + 3) {
            lim.begin(t0).ok();
            lim.end_failure(t0);
        }
        assert!(lim.begin(t0).is_err());
        assert!(lim.begin(t0 + RESET_AFTER).is_ok());
        assert_eq!(lim.failures, 0);
    }

    #[test]
    fn limita_handshakes_simultaneos() {
        let mut lim = AttemptLimiter::default();
        let t0 = Instant::now();
        for _ in 0..MAX_IN_PROGRESS {
            assert!(lim.begin(t0).is_ok());
        }
        assert_eq!(lim.begin(t0), Err(Refused::Busy));
        lim.end_success();
        assert!(lim.begin(t0).is_ok());
    }

    #[test]
    fn acerto_nao_zera_as_falhas() {
        let mut lim = AttemptLimiter::default();
        let t0 = Instant::now();
        for _ in 0..FREE_FAILURES {
            falhar(&mut lim, t0);
        }
        lim.begin(t0).unwrap();
        lim.end_success();
        falhar(&mut lim, t0); // 6ª falha ainda bloqueia
        assert!(matches!(lim.begin(t0), Err(Refused::Blocked(_))));
    }
}
