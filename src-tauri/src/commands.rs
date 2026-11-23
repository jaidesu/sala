//! Comandos que a interface pode chamar.
//!
//! Cada função marcada com `#[tauri::command]` vira algo que o frontend chama com
//! `invoke("nome_da_funcao", { argumentos })`. O valor de retorno é convertido
//! para JSON pelo `serde`, por isso os tipos de retorno derivam `Serialize`.
//!
//! Comandos `async` que recebem `State` precisam devolver `Result`; o lado
//! `Err(String)` chega no JavaScript como exceção, com a mensagem pronta para
//! mostrar ao usuário.

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::{Mutex, mpsc};

use crate::{
    net::{
        NetEvent, PathKind,
        endpoint::NetMode,
        guest::{Guest, PingResult},
        host::Host,
    },
    protocol::PROTOCOL_VERSION,
    room::invite::Invite,
};

/// Nome do evento Tauri que leva os `NetEvent` para a interface.
pub const NET_EVENT: &str = "net-event";

/// Estado de rede do app, guardado pelo Tauri (`.manage(...)` no lib.rs).
///
/// Usamos o `Mutex` do tokio (e não o da biblioteca padrão) porque ele pode
/// ficar travado enquanto esperamos operações de rede (`.await`).
#[derive(Default)]
pub struct NetState {
    host: Mutex<Option<Host>>,
    guest: Mutex<Option<Guest>>,
}

/// Informações básicas do app, mostradas no rodapé da tela inicial.
///
/// `rename_all = "camelCase"` faz o campo `protocol_version` (estilo Rust)
/// chegar no TypeScript como `protocolVersion` (estilo JS).
#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub name: &'static str,
    pub version: &'static str,
    pub protocol_version: u16,
}

/// Devolve nome e versão do app e a versão do protocolo de rede.
#[tauri::command]
pub fn app_info() -> AppInfo {
    AppInfo {
        name: "Sala",
        // `env!` lê a versão do Cargo.toml em tempo de compilação.
        version: env!("CARGO_PKG_VERSION"),
        protocol_version: PROTOCOL_VERSION,
    }
}

/// Cria um canal de eventos e repassa tudo o que chegar nele para a interface.
fn event_channel(app: AppHandle) -> mpsc::UnboundedSender<NetEvent> {
    let (tx, mut rx) = mpsc::unbounded_channel::<NetEvent>();
    tauri::async_runtime::spawn(async move {
        while let Some(event) = rx.recv().await {
            let _ = app.emit(NET_EVENT, event);
        }
    });
    tx
}

/// Abre uma sala e devolve o convite (texto para copiar).
#[tauri::command]
pub async fn host_start(
    mode: NetMode,
    app: AppHandle,
    state: State<'_, NetState>,
) -> Result<String, String> {
    let mut slot = state.host.lock().await;
    // Se já havia uma sala aberta, fecha antes de abrir outra.
    if let Some(old) = slot.take() {
        old.stop().await;
    }
    let host = Host::start(mode, event_channel(app))
        .await
        .map_err(|e| e.to_string())?;
    let invite = host.invite().encode();
    *slot = Some(host);
    Ok(invite)
}

/// Fecha a sala (desconecta todos os Convidados).
#[tauri::command]
pub async fn host_stop(state: State<'_, NetState>) -> Result<(), String> {
    if let Some(host) = state.host.lock().await.take() {
        host.stop().await;
    }
    Ok(())
}

/// Entra na sala do convite. Devolve o caminho inicial (direto ou relay).
#[tauri::command]
pub async fn guest_join(
    invite: String,
    mode: NetMode,
    app: AppHandle,
    state: State<'_, NetState>,
) -> Result<PathKind, String> {
    let invite = Invite::decode(&invite).map_err(|e| e.to_string())?;
    let mut slot = state.guest.lock().await;
    if let Some(old) = slot.take() {
        old.leave().await;
    }
    let guest = Guest::connect(mode, &invite, event_channel(app))
        .await
        .map_err(|e| e.to_string())?;
    let path = guest.path();
    *slot = Some(guest);
    Ok(path)
}

/// Mede o tempo de ida e volta até o Host.
#[tauri::command]
pub async fn guest_ping(state: State<'_, NetState>) -> Result<PingResult, String> {
    let mut slot = state.guest.lock().await;
    let guest = slot
        .as_mut()
        .ok_or("Você não está conectado a nenhuma sala.")?;
    guest.ping().await.map_err(|e| e.to_string())
}

/// Sai da sala.
#[tauri::command]
pub async fn guest_leave(state: State<'_, NetState>) -> Result<(), String> {
    if let Some(guest) = state.guest.lock().await.take() {
        guest.leave().await;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_info_usa_versao_do_cargo_e_do_protocolo() {
        let info = app_info();
        assert_eq!(info.name, "Sala");
        assert_eq!(info.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(info.protocol_version, PROTOCOL_VERSION);
    }

    #[test]
    fn app_info_vira_json_em_camel_case() {
        let json = serde_json::to_value(app_info()).unwrap();
        assert!(json.get("protocolVersion").is_some());
        assert!(json.get("protocol_version").is_none());
    }
}
