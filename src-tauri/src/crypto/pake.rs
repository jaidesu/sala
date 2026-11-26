//! Handshake de senha: Argon2id + SPAKE2 + confirmação de chave.
//!
//! # Ideia geral
//!
//! Queremos que só entre na sala quem sabe a senha, **sem a senha trafegar**
//! e sem que alguém que grave o tráfego consiga testar senhas depois, com
//! calma, no próprio computador (ataque de dicionário "offline").
//!
//! 1. **Argon2id**: cada lado transforma a senha em 32 bytes. O cálculo é
//!    propositalmente caro (64 MiB de memória, 3 passadas). O "sal" inclui o
//!    id da sala, então a mesma senha vira valores diferentes em salas diferentes.
//! 2. **SPAKE2**: cada lado manda uma mensagem de 33 bytes derivada dos 32
//!    bytes acima + um número aleatório. Os dois chegam à mesma chave secreta
//!    **só se** usaram a mesma senha. As mensagens não revelam nada que permita
//!    testar senhas offline: cada palpite exige uma nova conversa com o Host.
//! 3. **Amarração ao canal**: as "identidades" do SPAKE2 incluem a chave pública
//!    do Host, a do Convidado, o id da sala e um valor exportado da própria
//!    sessão TLS do QUIC. Mensagens de uma conexão não servem em outra.
//! 4. **Confirmação de chave**: cada lado manda um HMAC provando que tem a
//!    chave. O Convidado prova primeiro; o Host só responde se a prova bater.
//!
//! Papéis do SPAKE2: o Convidado é o lado "A" e o Host o lado "B".

use anyhow::{Result, anyhow};
use argon2::{Algorithm, Argon2, Params, Version};
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use spake2::{Ed25519Group, Identity, Password, Spake2};
use zeroize::Zeroizing;

use crate::room::invite::RoomId;

/// Memória usada pelo Argon2id, em KiB (64 MiB).
const ARGON2_M_COST_KIB: u32 = 64 * 1024;
/// Número de passadas do Argon2id.
const ARGON2_T_COST: u32 = 3;
/// Paralelismo do Argon2id (1 = uma thread; o i3 tem poucos núcleos).
const ARGON2_P_COST: u32 = 1;
/// Prefixo do sal do Argon2id (seguido do id da sala).
const ARGON2_SALT_PREFIX: &[u8] = b"sala/argon2id/v1/";

/// Rótulo usado para exportar o valor de amarração da sessão TLS.
pub const EXPORTER_LABEL: &[u8] = b"EXPORTER-sala-pake-v1";

const ID_GUEST_PREFIX: &[u8] = b"sala/pake/v1/guest/";
const ID_HOST_PREFIX: &[u8] = b"sala/pake/v1/host/";
const CONFIRM_GUEST: &[u8] = b"sala/confirm/v1/guest";
const CONFIRM_HOST: &[u8] = b"sala/confirm/v1/host";

type HmacSha256 = Hmac<Sha256>;

/// Resultado do Argon2id. `Zeroizing` apaga os bytes da memória ao descartar.
pub type StretchedPassword = Zeroizing<[u8; 32]>;

/// Aplica o Argon2id à senha. Leva cerca de 1 s; chame fora da thread
/// principal (`tokio::task::spawn_blocking`).
pub fn stretch_password(password: &str, room_id: &RoomId) -> Result<StretchedPassword> {
    let params = Params::new(ARGON2_M_COST_KIB, ARGON2_T_COST, ARGON2_P_COST, Some(32))
        .map_err(|e| anyhow!("parâmetros do Argon2 inválidos: {e}"))?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);

    let mut salt = ARGON2_SALT_PREFIX.to_vec();
    salt.extend_from_slice(&room_id.0);

    let mut out = Zeroizing::new([0u8; 32]);
    argon
        .hash_password_into(password.as_bytes(), &salt, out.as_mut())
        .map_err(|e| anyhow!("falha no Argon2: {e}"))?;
    Ok(out)
}

/// Quem está rodando o handshake.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Guest,
    Host,
}

impl Role {
    fn peer(self) -> Self {
        match self {
            Self::Guest => Self::Host,
            Self::Host => Self::Guest,
        }
    }

    fn confirm_label(self) -> &'static [u8] {
        match self {
            Self::Guest => CONFIRM_GUEST,
            Self::Host => CONFIRM_HOST,
        }
    }
}

/// Tudo o que amarra o handshake a esta conexão específica.
#[derive(Debug, Clone)]
pub struct PakeContext {
    pub room_id: RoomId,
    /// Chave pública (EndpointId) do Host.
    pub host_id: [u8; 32],
    /// Chave pública (EndpointId) do Convidado.
    pub guest_id: [u8; 32],
    /// Valor exportado da sessão TLS (igual nos dois lados da mesma conexão).
    pub channel_binding: [u8; 32],
}

impl PakeContext {
    /// Identidade do lado A (Convidado).
    fn id_guest(&self) -> Vec<u8> {
        [ID_GUEST_PREFIX, &self.guest_id].concat()
    }

    /// Identidade do lado B (Host), com sala e amarração ao canal.
    fn id_host(&self) -> Vec<u8> {
        [
            ID_HOST_PREFIX,
            &self.host_id,
            &self.room_id.0,
            &self.channel_binding,
        ]
        .concat()
    }

    /// Tudo junto, para entrar no HMAC de confirmação. Os campos têm tamanho
    /// fixo, então a concatenação não é ambígua.
    fn transcript(&self) -> Vec<u8> {
        [self.id_guest(), self.id_host()].concat()
    }
}

/// Handshake em andamento: já mandamos nossa mensagem, falta a do outro lado.
pub struct PakeState {
    spake: Spake2<Ed25519Group>,
    role: Role,
    transcript: Vec<u8>,
}

/// Começa o handshake. Devolve o estado e a mensagem a enviar ao outro lado.
pub fn start(stretched: &[u8; 32], ctx: &PakeContext, role: Role) -> (PakeState, Vec<u8>) {
    let password = Password::new(stretched);
    let id_a = Identity::new(&ctx.id_guest());
    let id_b = Identity::new(&ctx.id_host());
    let (spake, outbound) = match role {
        Role::Guest => Spake2::<Ed25519Group>::start_a(&password, &id_a, &id_b),
        Role::Host => Spake2::<Ed25519Group>::start_b(&password, &id_a, &id_b),
    };
    let state = PakeState {
        spake,
        role,
        transcript: ctx.transcript(),
    };
    (state, outbound)
}

impl PakeState {
    /// Recebe a mensagem do outro lado e calcula a chave da sessão.
    ///
    /// Atenção: com senha errada isto **não** dá erro; dá uma chave diferente.
    /// Quem descobre o problema é a confirmação (`verify_peer`).
    pub fn finish(self, inbound: &[u8]) -> Result<SessionKey> {
        let key = self
            .spake
            .finish(inbound)
            .map_err(|_| anyhow!("mensagem do handshake inválida"))?;
        Ok(SessionKey {
            key: Zeroizing::new(key),
            role: self.role,
            transcript: self.transcript,
        })
    }
}

/// Chave combinada pelo SPAKE2. Usada só para a confirmação.
pub struct SessionKey {
    key: Zeroizing<Vec<u8>>,
    role: Role,
    transcript: Vec<u8>,
}

impl SessionKey {
    fn mac(&self, role: Role) -> HmacSha256 {
        let mut mac =
            HmacSha256::new_from_slice(&self.key).expect("HMAC aceita chave de qualquer tamanho");
        mac.update(role.confirm_label());
        mac.update(&self.transcript);
        mac
    }

    /// Nossa prova de que temos a chave (para enviar ao outro lado).
    pub fn my_confirmation(&self) -> Vec<u8> {
        self.mac(self.role).finalize().into_bytes().to_vec()
    }

    /// Confere a prova do outro lado. A comparação é de tempo constante, para
    /// não vazar informação pela demora da resposta.
    pub fn verify_peer(&self, confirmation: &[u8]) -> bool {
        self.mac(self.role.peer())
            .verify_slice(confirmation)
            .is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contexto() -> PakeContext {
        PakeContext {
            room_id: RoomId([1; 16]),
            host_id: [2; 32],
            guest_id: [3; 32],
            channel_binding: [4; 32],
        }
    }

    /// Roda o handshake completo e devolve (host_aceitou, convidado_aceitou).
    fn handshake(
        pw_guest: &str,
        pw_host: &str,
        ctx_guest: &PakeContext,
        ctx_host: &PakeContext,
    ) -> (bool, bool) {
        let sg = stretch_password(pw_guest, &ctx_guest.room_id).unwrap();
        let sh = stretch_password(pw_host, &ctx_host.room_id).unwrap();

        let (guest, msg_g) = start(&sg, ctx_guest, Role::Guest);
        let (host, msg_h) = start(&sh, ctx_host, Role::Host);

        let kg = guest.finish(&msg_h).unwrap();
        let kh = host.finish(&msg_g).unwrap();

        let host_ok = kh.verify_peer(&kg.my_confirmation());
        let guest_ok = kg.verify_peer(&kh.my_confirmation());
        (host_ok, guest_ok)
    }

    #[test]
    fn mesma_senha_confirma_dos_dois_lados() {
        let ctx = contexto();
        assert_eq!(handshake("cavalo bateria", "cavalo bateria", &ctx, &ctx), (true, true));
    }

    #[test]
    fn senha_errada_falha_dos_dois_lados() {
        let ctx = contexto();
        assert_eq!(handshake("cavalo bateria", "cavalo baterla", &ctx, &ctx), (false, false));
    }

    #[test]
    fn conexao_diferente_falha_mesmo_com_senha_certa() {
        // Simula um atacante repassando mensagens entre duas conexões.
        let ctx = contexto();
        let mut outro = contexto();
        outro.channel_binding = [9; 32];
        assert_eq!(handshake("senha certa!", "senha certa!", &ctx, &outro), (false, false));
    }

    #[test]
    fn convidado_nao_aceita_a_propria_prova_de_volta() {
        // Um Host falso que só "ecoa" a prova do Convidado não deve passar.
        let ctx = contexto();
        let s = stretch_password("qualquer", &ctx.room_id).unwrap();
        let (guest, _msg_g) = start(&s, &ctx, Role::Guest);
        let (_host, msg_h) = start(&s, &ctx, Role::Host);
        let kg = guest.finish(&msg_h).unwrap();
        assert!(!kg.verify_peer(&kg.my_confirmation()));
    }

    #[test]
    fn mensagem_de_handshake_lixo_da_erro() {
        let ctx = contexto();
        let s = stretch_password("qualquer", &ctx.room_id).unwrap();
        let (guest, _) = start(&s, &ctx, Role::Guest);
        assert!(guest.finish(b"lixo").is_err());
    }

    #[test]
    fn argon2_depende_da_senha_e_da_sala() {
        let a = stretch_password("senha", &RoomId([1; 16])).unwrap();
        let b = stretch_password("senha", &RoomId([1; 16])).unwrap();
        let c = stretch_password("senha", &RoomId([2; 16])).unwrap();
        let d = stretch_password("senhA", &RoomId([1; 16])).unwrap();
        assert_eq!(*a, *b);
        assert_ne!(*a, *c);
        assert_ne!(*a, *d);
    }
}
