//! Criação do endpoint do iroh.
//!
//! Um `Endpoint` é o "ponto de rede" do app: tem uma chave própria (a
//! identidade do nó), abre uma porta UDP e sabe conversar com os relays.
//! Cada execução gera uma chave nova; por isso um convite antigo deixa de
//! funcionar quando o Host fecha o app.

use std::time::Duration;

use anyhow::{Result, anyhow};
use iroh::{Endpoint, EndpointAddr, endpoint::presets};
use serde::Deserialize;

/// Identificador do protocolo (ALPN) negociado no handshake do QUIC.
///
/// Se Host e Convidado não falarem o mesmo ALPN, a conexão nem abre. O `/1`
/// permite mudanças incompatíveis no futuro sem confusão.
pub const ALPN: &[u8] = b"sala/1";

/// Quanto esperar o Host ficar "online" (conectado a um relay).
const ONLINE_TIMEOUT: Duration = Duration::from_secs(20);

/// Como o app usa a rede.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NetMode {
    /// Padrão: relays e descoberta de endereço do n0 (funciona pela internet,
    /// atrás de NAT, sem configurar nada).
    Internet,
    /// Só rede local: sem relay e sem descoberta. Útil para testes e para usar
    /// numa rede sem internet. O convite passa a conter os IPs locais do Host.
    LocalOnly,
}

/// Cria um endpoint. `accept = true` para o Host (que aceita conexões).
pub async fn bind(mode: NetMode, accept: bool) -> Result<Endpoint> {
    let builder = match mode {
        NetMode::Internet => Endpoint::builder(presets::N0),
        // `Minimal` não configura relay nem descoberta: só conexões por IP.
        NetMode::LocalOnly => Endpoint::builder(presets::Minimal),
    };
    let builder = if accept {
        builder.alpns(vec![ALPN.to_vec()])
    } else {
        builder
    };
    builder
        .bind()
        .await
        .map_err(|e| anyhow!("Não foi possível abrir a conexão de rede: {e}"))
}

/// O endereço do Host que vai dentro do convite.
///
/// - Internet: espera conectar a um relay e coloca **só** a chave pública e o
///   relay. Os IPs não vão no convite (evita expor o IP de quem cria a sala a
///   quem só viu o convite); a conexão direta é negociada depois, pelo iroh.
/// - Só rede local: coloca a chave e os IPs locais.
pub async fn invite_addr(endpoint: &Endpoint, mode: NetMode) -> Result<EndpointAddr> {
    match mode {
        NetMode::Internet => {
            tokio::time::timeout(ONLINE_TIMEOUT, endpoint.online())
                .await
                .map_err(|_| {
                    anyhow!("Sem acesso à internet: não foi possível alcançar um servidor relay.")
                })?;
            let addr = endpoint.addr();
            let relays = addr.addrs.into_iter().filter(|a| a.is_relay());
            Ok(EndpointAddr::from_parts(addr.id, relays))
        }
        NetMode::LocalOnly => {
            let addr = endpoint.addr();
            let ips = addr.addrs.into_iter().filter(|a| a.is_ip());
            Ok(EndpointAddr::from_parts(addr.id, ips))
        }
    }
}
