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
//! - `room`     — convite, participantes, histórico e regras da sala.
//! - `crypto`   — Argon2id + SPAKE2, limite de tentativas, geração de senha.

mod commands;
mod crypto;
mod net;
mod protocol;
mod room;

use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

use tauri::{AppHandle, Manager, RunEvent, WindowEvent};

/// Vira `true` quando o encerramento educado começa (evita fazer duas vezes).
static SHUTTING_DOWN: AtomicBool = AtomicBool::new(false);

/// Avisa os outros (sala encerrada / saí da sala) e só então fecha o app.
/// Se a rede travar, desiste depois de 3 s e fecha mesmo assim.
fn graceful_exit(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<commands::NetState>();
        let _ = tokio::time::timeout(Duration::from_secs(3), state.shutdown()).await;
        app.exit(0);
    });
}

/// Ponto de entrada do aplicativo, chamado pelo `main.rs`.
pub fn run() {
    let app = tauri::Builder::default()
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
            commands::guest_leave,
            commands::send_message,
            commands::kick_participant,
        ])
        // Fechar a janela (X): segura o fechamento até avisar os outros.
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event
                && !SHUTTING_DOWN.swap(true, Ordering::SeqCst)
            {
                api.prevent_close();
                graceful_exit(window.app_handle());
            }
        })
        .build(tauri::generate_context!())
        .expect("falha ao iniciar o aplicativo Tauri");

    // Sair pelo sistema (ex.: Cmd+Q no macOS) também passa pelo encerramento
    // educado. `code` vem preenchido quando somos nós que chamamos `exit`.
    app.run(|app, event| {
        if let RunEvent::ExitRequested { api, code, .. } = event
            && code.is_none()
            && !SHUTTING_DOWN.swap(true, Ordering::SeqCst)
        {
            api.prevent_exit();
            graceful_exit(app);
        }
    });
}
