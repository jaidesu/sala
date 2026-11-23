//! Núcleo do Sala.
//!
//! Tudo o que envolve rede, criptografia e estado da sala vive aqui, em Rust.
//! A interface (React) nunca toca na rede: ela só chama *comandos* deste núcleo
//! (via `invoke`) e escuta *eventos* que ele emite (via `listen`).
//!
//! Mapa dos módulos (vão sendo criados conforme as fases do SPEC.md):
//! - `commands` — funções expostas à interface.
//! - `protocol` — formato das mensagens trocadas entre Host e Convidados.
//! - `net`      — (fase 2) endpoint iroh, lado Host e lado Convidado.
//! - `crypto`   — (fase 3) Argon2id + SPAKE2 e chaves de sessão.
//! - `room`     — (fase 4) participantes, histórico, convite.

mod commands;
mod protocol;

/// Ponto de entrada do aplicativo, chamado pelo `main.rs`.
pub fn run() {
    tauri::Builder::default()
        // Lista de comandos que a interface pode chamar com `invoke("nome")`.
        // Comando que não está aqui simplesmente não existe para a interface.
        .invoke_handler(tauri::generate_handler![commands::app_info])
        .run(tauri::generate_context!())
        .expect("falha ao iniciar o aplicativo Tauri");
}
