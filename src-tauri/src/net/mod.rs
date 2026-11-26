//! Rede: conexão entre Host e Convidados usando o iroh.
//!
//! - `endpoint`: cria o "ponto de rede" do iroh e escolhe o modo (internet ou
//!   só rede local).
//! - `host`: aceita conexões de Convidados.
//! - `guest`: conecta ao Host a partir de um convite.
//! - `auth`: handshake de senha (PAKE) sobre os streams da conexão.
//!
//! As camadas de cima (comandos Tauri) recebem notificações por um canal
//! (`mpsc`) de `NetEvent`. Assim este módulo não depende do Tauri e pode ser
//! testado sozinho.

pub mod auth;
pub mod endpoint;
pub mod guest;
pub mod host;

use std::time::Duration;

use serde::Serialize;

/// Quanto esperar pelo `Hello` do outro lado antes de desistir.
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

/// Por onde os dados estão passando agora.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PathKind {
    /// Conexão direta entre os dois computadores (hole punching deu certo).
    Direct,
    /// Passando pelo servidor relay (cifrado; o relay não lê o conteúdo).
    Relay,
    /// Ainda não há caminho escolhido.
    Unknown,
}

impl PathKind {
    /// Decide o tipo de caminho a partir da lista de caminhos da conexão.
    ///
    /// Recebe `(selecionado, é_ip, é_relay)` de cada caminho, em vez dos tipos
    /// do iroh, para ficar fácil de testar.
    pub fn classify(paths: impl Iterator<Item = (bool, bool, bool)>) -> Self {
        for (selected, is_ip, is_relay) in paths {
            if selected {
                if is_ip {
                    return Self::Direct;
                }
                if is_relay {
                    return Self::Relay;
                }
            }
        }
        Self::Unknown
    }

    /// Caminho em uso agora numa conexão do iroh.
    pub fn of(conn: &iroh::endpoint::Connection) -> Self {
        let paths = conn.paths();
        Self::classify(
            paths
                .iter()
                .map(|p| (p.is_selected(), p.is_ip(), p.is_relay())),
        )
    }
}

/// Notificações da rede para a interface.
///
/// Vira JSON com o campo `kind`, por exemplo
/// `{"kind":"pathChanged","path":"direct"}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum NetEvent {
    /// (Host) Um Convidado completou o handshake.
    GuestConnected { guest: String },
    /// (Host) Um Convidado saiu ou caiu.
    GuestDisconnected { guest: String },
    /// (Host) Alguém tentou entrar e errou a senha (ou desistiu no meio).
    JoinRefused,
    /// (Convidado) O caminho até o Host mudou (ex.: relay → direto).
    PathChanged { path: PathKind },
    /// (Convidado) A conexão com o Host acabou.
    Disconnected,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifica_caminho_selecionado() {
        // (selecionado, ip, relay)
        let direto = [(false, false, true), (true, true, false)];
        assert_eq!(PathKind::classify(direto.into_iter()), PathKind::Direct);

        let relay = [(true, false, true), (false, true, false)];
        assert_eq!(PathKind::classify(relay.into_iter()), PathKind::Relay);

        let nenhum = [(false, true, false)];
        assert_eq!(PathKind::classify(nenhum.into_iter()), PathKind::Unknown);
        assert_eq!(PathKind::classify(std::iter::empty()), PathKind::Unknown);
    }

    #[test]
    fn evento_vira_json_com_kind() {
        let ev = NetEvent::PathChanged {
            path: PathKind::Direct,
        };
        assert_eq!(
            serde_json::to_string(&ev).unwrap(),
            r#"{"kind":"pathChanged","path":"direct"}"#
        );
    }

    /// Teste de integração: Host e Convidado de verdade, no mesmo computador,
    /// conversando pelo iroh em modo "só rede local" (sem internet/relay).
    mod integracao {
        use std::time::Duration;

        use tokio::sync::mpsc;
        use tokio::time::timeout;
        use zeroize::Zeroizing;

        use super::super::{NetEvent, PathKind, endpoint::NetMode, guest::Guest, host::Host};
        use crate::{
            crypto::limiter::FREE_FAILURES,
            room::invite::{Invite, RoomId},
        };

        const LIMITE: Duration = Duration::from_secs(30);
        const SENHA: &str = "senha-de-teste-123";

        fn senha(s: &str) -> Zeroizing<String> {
            Zeroizing::new(s.to_string())
        }

        async fn abrir_sala() -> (Host, mpsc::UnboundedReceiver<NetEvent>) {
            let (tx, rx) = mpsc::unbounded_channel();
            let host = Host::start(NetMode::LocalOnly, senha(SENHA), tx)
                .await
                .unwrap();
            (host, rx)
        }

        async fn entrar(invite: &Invite, pw: &str) -> anyhow::Result<Guest> {
            let (tx, _rx) = mpsc::unbounded_channel();
            timeout(
                LIMITE,
                Guest::connect(NetMode::LocalOnly, invite, senha(pw), tx),
            )
            .await
            .expect("tentativa demorou demais")
        }

        async fn proximo_evento(rx: &mut mpsc::UnboundedReceiver<NetEvent>) -> NetEvent {
            timeout(LIMITE, rx.recv())
                .await
                .expect("evento não chegou a tempo")
                .expect("canal fechado")
        }

        #[tokio::test]
        async fn convidado_conecta_e_faz_ping() {
            let (host, mut host_rx) = abrir_sala().await;

            // O convite passa por texto, como aconteceria de verdade.
            let invite = Invite::decode(&host.invite().encode()).unwrap();
            let mut guest = entrar(&invite, SENHA).await.unwrap();

            assert!(matches!(
                proximo_evento(&mut host_rx).await,
                NetEvent::GuestConnected { .. }
            ));

            for _ in 0..3 {
                let pong = guest.ping().await.unwrap();
                // Sem relay, o único caminho possível é o direto.
                assert_eq!(pong.path, PathKind::Direct);
            }

            guest.leave().await;
            assert!(matches!(
                proximo_evento(&mut host_rx).await,
                NetEvent::GuestDisconnected { .. }
            ));

            host.stop().await;
        }

        #[tokio::test]
        async fn convite_de_outra_sala_e_recusado() {
            let (host, mut host_rx) = abrir_sala().await;

            // Mesmo endereço do Host, mas outro identificador de sala.
            let mut invite = host.invite().clone();
            invite.room_id = RoomId::random();

            assert!(entrar(&invite, SENHA).await.is_err());
            // O Host não deve ter anunciado ninguém.
            assert!(host_rx.try_recv().is_err());
            host.stop().await;
        }

        #[tokio::test]
        async fn senha_errada_e_recusada() {
            let (host, mut host_rx) = abrir_sala().await;
            let invite = host.invite().clone();

            let erro = entrar(&invite, "senha-errada").await.err().unwrap();
            // Mensagem genérica: não diz que foi a senha.
            assert!(!erro.to_string().to_lowercase().contains("incorreta"));
            assert_eq!(proximo_evento(&mut host_rx).await, NetEvent::JoinRefused);

            // E quem sabe a senha continua entrando normalmente.
            let guest = entrar(&invite, SENHA).await.unwrap();
            assert!(matches!(
                proximo_evento(&mut host_rx).await,
                NetEvent::GuestConnected { .. }
            ));
            guest.leave().await;
            host.stop().await;
        }

        #[tokio::test]
        async fn muitas_falhas_bloqueiam_ate_a_senha_certa() {
            let (host, mut host_rx) = abrir_sala().await;
            let invite = host.invite().clone();

            // Falhas livres + 1: a última já ativa o bloqueio (2 s).
            for _ in 0..=FREE_FAILURES {
                assert!(entrar(&invite, "chute-errado").await.is_err());
                assert_eq!(proximo_evento(&mut host_rx).await, NetEvent::JoinRefused);
            }
            // Mais duas rodadas para o bloqueio crescer: 4 s e depois 8 s.
            for espera in [2, 4] {
                tokio::time::sleep(Duration::from_secs(espera) + Duration::from_millis(300)).await;
                assert!(entrar(&invite, "chute-errado").await.is_err());
                assert_eq!(proximo_evento(&mut host_rx).await, NetEvent::JoinRefused);
            }

            // Bloqueado por 8 s: nem a senha certa entra agora.
            assert!(entrar(&invite, SENHA).await.is_err());
            // Recusa por bloqueio não gera evento (evita inundar o registro).
            assert!(host_rx.try_recv().is_err());
            host.stop().await;
        }
    }
}
