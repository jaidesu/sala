//! Senhas: geração de senha forte e regra mínima.
//!
//! A senha gerada tem 16 caracteres aleatórios em 4 grupos, por exemplo
//! `k7mq-x2pd-9tfa-wr4h`. O alfabeto tem 31 símbolos e evita os que se
//! confundem ao ditar ou ler (`0/o`, `1/l/i`). Isso dá cerca de 79 bits de
//! entropia: fora de alcance mesmo sem o limite de tentativas.
//!
//! (O SPEC.md sugere 5–6 palavras. Uma lista de palavras em português boa e
//! revisada é grande; os grupos de caracteres dão segurança equivalente e
//! continuam fáceis de ditar. Dá para trocar depois sem mexer no resto.)

use anyhow::{Result, bail};

/// Tamanho mínimo aceito ao criar uma sala, em caracteres.
pub const MIN_PASSWORD_CHARS: usize = 8;

const ALPHABET: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";
const GROUPS: usize = 4;
const GROUP_LEN: usize = 4;

/// Gera uma senha forte com o gerador aleatório do sistema.
pub fn generate() -> String {
    let n = ALPHABET.len();
    // "Amostragem por rejeição": só aceitamos bytes abaixo do maior múltiplo de
    // `n` que cabe em 256. Assim todo símbolo tem exatamente a mesma chance
    // (usar `byte % n` direto favoreceria os primeiros símbolos).
    let limit = (256 / n) * n;

    let mut out = String::with_capacity(GROUPS * (GROUP_LEN + 1));
    let mut chosen = 0;
    while chosen < GROUPS * GROUP_LEN {
        let mut buf = [0u8; 32];
        getrandom::fill(&mut buf).expect("gerador aleatório do sistema indisponível");
        for &b in &buf {
            if chosen == GROUPS * GROUP_LEN {
                break;
            }
            if (b as usize) < limit {
                if chosen > 0 && chosen % GROUP_LEN == 0 {
                    out.push('-');
                }
                out.push(ALPHABET[b as usize % n] as char);
                chosen += 1;
            }
        }
    }
    out
}

/// Confere a senha escolhida pelo Host ao criar a sala.
pub fn validate_new(password: &str) -> Result<()> {
    if password.chars().count() < MIN_PASSWORD_CHARS {
        bail!("A senha precisa ter pelo menos {MIN_PASSWORD_CHARS} caracteres.");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formato_da_senha_gerada() {
        let p = generate();
        assert_eq!(p.len(), 19);
        let grupos: Vec<&str> = p.split('-').collect();
        assert_eq!(grupos.len(), GROUPS);
        for g in grupos {
            assert_eq!(g.len(), GROUP_LEN);
            assert!(g.bytes().all(|c| ALPHABET.contains(&c)));
        }
    }

    #[test]
    fn senhas_geradas_sao_diferentes() {
        assert_ne!(generate(), generate());
    }

    #[test]
    fn valida_tamanho_minimo() {
        assert!(validate_new("curta").is_err());
        assert!(validate_new("12345678").is_ok());
        // Conta caracteres, não bytes: "ç" ocupa 2 bytes em UTF-8.
        assert!(validate_new("çççç").is_err());
    }
}
