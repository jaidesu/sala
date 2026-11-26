/**
 * Ponte tipada entre a interface e o núcleo Rust.
 *
 * Regra do projeto: nenhum componente chama `invoke`/`listen` diretamente.
 * Tudo passa por aqui, para que os nomes dos comandos e os tipos fiquem num
 * lugar só. Os tipos abaixo precisam bater com os do Rust
 * (src-tauri/src/commands.rs e src-tauri/src/net/).
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

/** `PingResult` do Rust. */
export interface PingResult {
  rttMs: number;
  path: PathKind;
}

/** `NetEvent` do Rust (o campo `kind` diz qual é). */
export type NetEvent =
  | { kind: "guestConnected"; guest: string }
  | { kind: "guestDisconnected"; guest: string }
  | { kind: "joinRefused" }
  | { kind: "pathChanged"; path: PathKind }
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

/** Abre uma sala protegida por `password` e devolve o convite. */
export function hostStart(mode: NetMode, password: string): Promise<string> {
  return invoke<string>("host_start", { mode, password });
}

/** Fecha a sala. */
export function hostStop(): Promise<void> {
  return invoke("host_stop");
}

/** Entra na sala do convite usando a senha. Devolve o caminho inicial. */
export function guestJoin(
  invite: string,
  password: string,
  mode: NetMode,
): Promise<PathKind> {
  return invoke<PathKind>("guest_join", { invite, password, mode });
}

/** Mede a latência até o Host. */
export function guestPing(): Promise<PingResult> {
  return invoke<PingResult>("guest_ping");
}

/** Sai da sala. */
export function guestLeave(): Promise<void> {
  return invoke("guest_leave");
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
