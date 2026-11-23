/**
 * Ponte tipada entre a interface e o núcleo Rust.
 *
 * Regra do projeto: nenhum componente chama `invoke` diretamente. Tudo passa
 * por aqui, para que os nomes dos comandos e os tipos fiquem num lugar só.
 * Os tipos abaixo precisam bater com os structs do Rust (src-tauri/src/commands.rs).
 */
import { invoke, isTauri } from "@tauri-apps/api/core";

/** Espelho do struct `AppInfo` do Rust (campos em camelCase). */
export interface AppInfo {
  name: string;
  version: string;
  protocolVersion: number;
}

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
