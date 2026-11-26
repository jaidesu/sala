/**
 * Sala de exemplo para o modo demonstração.
 *
 * Abra `npm run dev` e acesse http://localhost:1420/?demo para ver a tela da
 * sala no navegador, sem o núcleo Rust. Só funciona fora do app Tauri.
 */
import type { RoomView } from "./room";
import { runningInTauri } from "./tauri";

export const isDemo =
  !runningInTauri && new URLSearchParams(window.location.search).has("demo");

const t0 = Date.now() - 12 * 60_000;
const min = (n: number) => t0 + n * 60_000;

export const demoRoom: RoomView = {
  role: "host",
  invite: "sala1:exemplo",
  me: { id: 0, nickname: "Jair", isHost: true },
  participants: [
    { id: 0, nickname: "Jair", isHost: true },
    { id: 1, nickname: "Ana", isHost: false },
    { id: 2, nickname: "Bia", isHost: false },
  ],
  timeline: [
    { kind: "system", key: "s1", text: "Sala aberta.", tone: "info" },
    { kind: "system", key: "s2", text: "Ana entrou.", tone: "info" },
    {
      kind: "chat",
      key: "m1",
      message: { id: 1, from: 1, nickname: "Ana", text: "Oi! Cheguei 👋", timestamp: min(1) },
    },
    {
      kind: "chat",
      key: "m2",
      message: {
        id: 2,
        from: 0,
        nickname: "Jair",
        text: "Bem-vinda! A senha funcionou de primeira?",
        timestamp: min(2),
      },
    },
    { kind: "system", key: "s3", text: "Bia entrou.", tone: "info" },
    {
      kind: "chat",
      key: "m3",
      message: {
        id: 3,
        from: 2,
        nickname: "Bia",
        text: "Testando <b>HTML</b>: deve aparecer como texto puro.\nE quebra de linha também.",
        timestamp: min(5),
      },
    },
    { kind: "system", key: "s4", text: "Tentativa de entrada recusada.", tone: "warn" },
  ],
  ended: false,
};
