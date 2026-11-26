/**
 * Ponte tipada entre a interface e o núcleo Rust.
 *
 * Regra do projeto: nenhum componente chama `invoke`/`listen` diretamente.
 * Tudo passa por aqui, para que os nomes dos comandos e os tipos fiquem num
 * lugar só. Os tipos abaixo precisam bater com os do Rust
 * (src-tauri/src/commands.rs, src-tauri/src/net/ e src-tauri/src/room/state.rs).
 */
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/** Espelho do struct `AppInfo` do Rust (campos em camelCase). */
export interface AppInfo {
  name: string;
  version: string;
  protocolVersion: number;
}

/** `NetMode` do Rust: internet (com relay) ou só rede local. */
export type NetMode = "internet" | "localOnly";

/** `PathKind` do Rust: por onde os dados estão passando. */
export type PathKind = "direct" | "relay" | "unknown";

/** Mesmos limites de `room/state.rs`. */
export const MAX_NICKNAME_CHARS = 32;
export const MAX_MESSAGE_CHARS = 4000;

export interface Participant {
  id: number;
  nickname: string;
  isHost: boolean;
  /** `false` enquanto a conexão da pessoa está caída (esperando voltar). */
  online: boolean;
}

export interface ChatMessage {
  id: number;
  from: number;
  nickname: string;
  text: string;
  /** Horário do Host em milissegundos desde 1970. */
  timestamp: number;
}

/** Foto da sala ao criar/entrar. */
export interface RoomSnapshot {
  me: Participant;
  participants: Participant[];
  history: ChatMessage[];
}

export interface HostStarted {
  invite: string;
  room: RoomSnapshot;
}

export interface GuestJoined {
  path: PathKind;
  room: RoomSnapshot;
}

/** `NetEvent` do Rust (o campo `kind` diz qual é). */
export type NetEvent =
  | { kind: "participantJoined"; participant: Participant }
  | { kind: "participantUpdated"; participant: Participant }
  | { kind: "participantLeft"; participant: Participant; kicked: boolean }
  | { kind: "chat"; message: ChatMessage }
  | { kind: "rejected"; reason: string }
  | { kind: "joinRefused" }
  | { kind: "pathChanged"; path: PathKind }
  | { kind: "reconnecting" }
  | { kind: "resynced"; room: RoomSnapshot; path: PathKind }
  | { kind: "kicked" }
  | { kind: "roomClosed" }
  | { kind: "disconnected" };

/**
 * `true` quando a página está rodando dentro do app Tauri.
 * `false` quando aberta num navegador comum (ex.: `npm run dev`), útil para
 * desenvolver o visual sem precisar compilar o Rust.
 */
export const runningInTauri = isTauri();

/** Pergunta ao núcleo o nome/versão do app e a versão do protocolo. */
export async function getAppInfo(): Promise<AppInfo | null> {
  if (!runningInTauri) return null;
  return invoke<AppInfo>("app_info");
}

/** Gera uma senha forte no núcleo Rust (gerador aleatório do sistema). */
export function generatePassword(): Promise<string> {
  return invoke<string>("generate_password");
}

/** Abre uma sala protegida por `password`, com o Host usando `nickname`. */
export function hostStart(
  mode: NetMode,
  password: string,
  nickname: string,
): Promise<HostStarted> {
  return invoke<HostStarted>("host_start", { mode, password, nickname });
}

/** Encerra a sala. */
export function hostStop(): Promise<void> {
  return invoke("host_stop");
}

/** Entra na sala do convite. */
export function guestJoin(
  invite: string,
  password: string,
  nickname: string,
  mode: NetMode,
): Promise<GuestJoined> {
  return invoke<GuestJoined>("guest_join", { invite, password, nickname, mode });
}

/** Sai da sala. */
export function guestLeave(): Promise<void> {
  return invoke("guest_leave");
}

/** (Host) Remove um participante da sala. */
export function kickParticipant(id: number): Promise<void> {
  return invoke("kick_participant", { id });
}

/** Envia uma mensagem na sala atual (Host ou Convidado). */
export function sendMessage(text: string): Promise<void> {
  return invoke("send_message", { text });
}

/** Escuta os eventos de rede. Devolve a função que para de escutar. */
export function onNetEvent(handler: (event: NetEvent) => void): Promise<UnlistenFn> {
  return listen<NetEvent>("net-event", (e) => handler(e.payload));
}

/** Converte o erro de um `invoke` em texto para mostrar ao usuário. */
export function errorMessage(err: unknown): string {
  if (typeof err === "string") return err;
  if (err instanceof Error) return err.message;
  return "Erro inesperado.";
}
