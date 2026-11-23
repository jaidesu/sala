//! Comandos que a interface pode chamar.
//!
//! Cada função marcada com `#[tauri::command]` vira algo que o frontend chama com
//! `invoke("nome_da_funcao")`. O valor de retorno é convertido para JSON pelo
//! `serde`, por isso os tipos de retorno derivam `Serialize`.

use serde::Serialize;

use crate::protocol::PROTOCOL_VERSION;

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
///
/// Serve, na fase 1, para provar que a ponte interface ↔ núcleo funciona.
#[tauri::command]
pub fn app_info() -> AppInfo {
    AppInfo {
        name: "Sala",
        // `env!` lê a versão do Cargo.toml em tempo de compilação.
        version: env!("CARGO_PKG_VERSION"),
        protocol_version: PROTOCOL_VERSION,
    }
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
