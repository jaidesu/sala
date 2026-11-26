/**
 * Medidor de força de senha (estimativa simples, feita só na interface).
 *
 * Ideia: estimar quantos "bits de entropia" a senha tem, isto é, quantas
 * tentativas um atacante precisaria no pior caso (2^bits). Para cada tipo de
 * caractere usado (minúsculas, maiúsculas, dígitos, símbolos), o "alfabeto"
 * cresce; entropia ≈ comprimento × log2(tamanho do alfabeto).
 *
 * É uma estimativa otimista para senhas escolhidas por pessoas (que seguem
 * padrões), por isso aplicamos descontos para repetições, sequências e
 * senhas muito comuns. Não substitui a senha gerada, que é aleatória de verdade.
 */

/** Mesmo valor de `MIN_PASSWORD_CHARS` no Rust (crypto/password.rs). */
export const MIN_PASSWORD_CHARS = 8;

export type Strength = "tooShort" | "weak" | "fair" | "strong";

/** Algumas das senhas mais usadas (em minúsculas). */
const COMMON = new Set([
  "12345678",
  "123456789",
  "1234567890",
  "password",
  "senha123",
  "senha1234",
  "qwertyui",
  "abcdefgh",
  "11111111",
  "00000000",
  "iloveyou",
  "brasil123",
  "abc12345",
  "mudar123",
]);

/** Senha no formato gerado pelo app (`xxxx-xxxx-xxxx-xxxx`). */
const GENERATED = /^[a-hjkmnp-z2-9]{4}(-[a-hjkmnp-z2-9]{4}){3}$/;

export function estimateBits(password: string): number {
  if (GENERATED.test(password)) return 79; // 16 símbolos de um alfabeto de 31

  let pool = 0;
  if (/[a-z]/.test(password)) pool += 26;
  if (/[A-Z]/.test(password)) pool += 26;
  if (/[0-9]/.test(password)) pool += 10;
  if (/[^a-zA-Z0-9]/.test(password)) pool += 33;
  if (pool === 0) return 0;

  const chars = Array.from(password);
  // Desconto: caracteres repetidos e sequências (abc, 123) contam pela metade.
  let effective = 0;
  for (let i = 0; i < chars.length; i++) {
    const prev = i > 0 ? chars[i - 1].codePointAt(0)! : -10;
    const cur = chars[i].codePointAt(0)!;
    effective += Math.abs(cur - prev) <= 1 ? 0.5 : 1;
  }
  return Math.round(effective * Math.log2(pool));
}

export function strength(password: string): Strength {
  if (Array.from(password).length < MIN_PASSWORD_CHARS) return "tooShort";
  if (COMMON.has(password.toLowerCase())) return "weak";
  const bits = estimateBits(password);
  if (bits < 40) return "weak";
  if (bits < 60) return "fair";
  return "strong";
}
