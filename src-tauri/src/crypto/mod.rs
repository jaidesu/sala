//! Criptografia da autenticação por senha.
//!
//! - `pake`: Argon2id + SPAKE2 + confirmação de chave (o handshake de senha).
//! - `limiter`: limite de tentativas erradas (proteção contra força bruta).
//! - `password`: geração de senha forte e regras mínimas.
//!
//! Regra do projeto: nada de criptografia inventada. Aqui só combinamos
//! primitivas prontas e conhecidas, cada uma com um papel bem definido.

pub mod limiter;
pub mod pake;
pub mod password;
