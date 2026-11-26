//! Núcleo do Sala.
//!
//! Tudo o que envolve rede, criptografia e estado da sala vive aqui, em Rust.
//! A interface (React) nunca toca na rede: ela só chama *comandos* deste núcleo
//! (via `invoke`) e escuta *eventos* que ele emite (via `listen`).
//!
//! Mapa dos módulos (vão sendo criados conforme as fases do SPEC.md):
//! - `commands` — funções expostas à interface.
//! - `protocol` — formato das mensagens trocadas entre Host e Convidados.
//! - `net`      — endpoint iroh, lado Host e lado Convidado.
//! - `room`     — convite (e, na fase 4, participantes e histórico).
//! - `crypto`   — Argon2id + SPAKE2, limite de tentativas, geração de senha.

mod commands;
mod crypto;
mod net;
mod protocol;
mod room;

/// Ponto de entrada do aplicativo, chamado pelo `main.rs`.
pub fn run() {
    tauri::Builder::default()
        // Estado compartilhado entre os comandos (sala aberta, conexão ativa).
        .manage(commands::NetState::default())
        // Lista de comandos que a interface pode chamar com `invoke("nome")`.
        // Comando que não está aqui simplesmente não existe para a interface.
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::generate_password,
            commands::host_start,
            commands::host_stop,
            commands::guest_join,
            commands::guest_ping,
            commands::guest_leave,
        ])
        .run(tauri::generate_context!())
        .expect("falha ao iniciar o aplicativo Tauri");
}
