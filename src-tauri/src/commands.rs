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
use zeroize::Zeroizing;

use crate::{
    crypto::password,
    net::{NetEvent, PathKind, RoomSnapshot, endpoint::NetMode, guest::Guest, host::Host},
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

/// Resposta de `host_start`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostStarted {
    pub invite: String,
    pub room: RoomSnapshot,
}

/// Resposta de `guest_join`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuestJoined {
    pub path: PathKind,
    pub room: RoomSnapshot,
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

/// Gera uma senha forte (ex.: `k7mq-x2pd-9tfa-wr4h`).
#[tauri::command]
pub fn generate_password() -> String {
    password::generate()
}

/// Abre uma sala e devolve o convite (texto para copiar) e a foto da sala.
///
/// A senha chega como `String` e é embrulhada em `Zeroizing` na hora, para ser
/// apagada da memória assim que o Argon2id terminar.
#[tauri::command]
pub async fn host_start(
    mode: NetMode,
    password: String,
    nickname: String,
    app: AppHandle,
    state: State<'_, NetState>,
) -> Result<HostStarted, String> {
    let password = Zeroizing::new(password);
    let mut slot = state.host.lock().await;
    // Se já havia uma sala aberta, fecha antes de abrir outra.
    if let Some(old) = slot.take() {
        old.stop().await;
    }
    let host = Host::start(mode, password, &nickname, event_channel(app))
        .await
        .map_err(|e| e.to_string())?;
    let started = HostStarted {
        invite: host.invite().encode(),
        room: host.snapshot(),
    };
    *slot = Some(host);
    Ok(started)
}

/// Encerra a sala (avisa e desconecta todos os Convidados).
#[tauri::command]
pub async fn host_stop(state: State<'_, NetState>) -> Result<(), String> {
    if let Some(host) = state.host.lock().await.take() {
        host.stop().await;
    }
    Ok(())
}

/// Entra na sala do convite.
#[tauri::command]
pub async fn guest_join(
    invite: String,
    password: String,
    nickname: String,
    mode: NetMode,
    app: AppHandle,
    state: State<'_, NetState>,
) -> Result<GuestJoined, String> {
    let password = Zeroizing::new(password);
    let invite = Invite::decode(&invite).map_err(|e| e.to_string())?;
    let mut slot = state.guest.lock().await;
    if let Some(old) = slot.take() {
        old.leave().await;
    }
    let (guest, room) = Guest::connect(mode, &invite, password, &nickname, event_channel(app))
        .await
        .map_err(|e| e.to_string())?;
    let joined = GuestJoined {
        path: guest.path(),
        room,
    };
    *slot = Some(guest);
    Ok(joined)
}

/// Sai da sala.
#[tauri::command]
pub async fn guest_leave(state: State<'_, NetState>) -> Result<(), String> {
    if let Some(guest) = state.guest.lock().await.take() {
        guest.leave().await;
    }
    Ok(())
}

/// Envia uma mensagem na sala atual (como Host ou como Convidado).
///
/// A mensagem aparece na tela quando volta como evento `chat`, já com o
/// número e o horário oficiais dados pelo Host.
#[tauri::command]
pub async fn send_message(text: String, state: State<'_, NetState>) -> Result<(), String> {
    if let Some(host) = state.host.lock().await.as_ref() {
        return host.send(&text).map_err(|e| e.to_string());
    }
    if let Some(guest) = state.guest.lock().await.as_mut() {
        return guest.send(&text).await.map_err(|e| e.to_string());
    }
    Err("Você não está em nenhuma sala.".into())
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
