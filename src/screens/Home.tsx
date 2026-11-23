import { useEffect, useState } from "react";
import {
  getAppInfo,
  runningInTauri,
  type AppInfo,
  type NetMode,
} from "../lib/tauri";

interface HomeProps {
  /** Mensagem de erro da última tentativa (ex.: sem internet). */
  error: string | null;
  /** `true` enquanto a sala está sendo aberta. */
  busy: boolean;
  onCreate: (mode: NetMode) => void;
  onJoin: (mode: NetMode) => void;
}

/**
 * Tela inicial: escolher entre criar uma sala ou entrar em uma.
 *
 * A opção "só rede local" desliga relays e descoberta pela internet. Serve
 * para testar em dois computadores na mesma rede, mesmo sem internet.
 */
export default function Home({ error, busy, onCreate, onJoin }: HomeProps) {
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [localOnly, setLocalOnly] = useState(false);
  const mode: NetMode = localOnly ? "localOnly" : "internet";
  const disabled = !runningInTauri || busy;

  useEffect(() => {
    getAppInfo()
      .then(setInfo)
      .catch(() => setInfo(null));
  }, []);

  return (
    <main className="flex h-full flex-col items-center justify-center gap-10 p-6">
      <header className="text-center">
        <h1 className="text-4xl font-semibold tracking-tight">Sala</h1>
        <p className="mt-2 text-zinc-400">
          Chat privado, direto entre computadores. Sem servidor, sem cadastro.
        </p>
      </header>

      <div className="flex w-full max-w-xs flex-col gap-3">
        <button
          type="button"
          disabled={disabled}
          onClick={() => onCreate(mode)}
          className="rounded-lg bg-emerald-600 px-4 py-3 font-medium text-white hover:bg-emerald-500 disabled:cursor-not-allowed disabled:opacity-50"
        >
          {busy ? "Abrindo sala…" : "Criar sala"}
        </button>
        <button
          type="button"
          disabled={disabled}
          onClick={() => onJoin(mode)}
          className="rounded-lg border border-zinc-700 px-4 py-3 font-medium hover:bg-zinc-900 disabled:cursor-not-allowed disabled:opacity-50"
        >
          Entrar em uma sala
        </button>

        <label className="mt-1 flex items-center gap-2 text-sm text-zinc-400">
          <input
            type="checkbox"
            checked={localOnly}
            onChange={(e) => setLocalOnly(e.target.checked)}
            className="accent-emerald-500"
          />
          Só rede local (sem relay)
        </label>

        {error && (
          <p role="alert" className="rounded-md bg-red-950 px-3 py-2 text-sm text-red-300">
            {error}
          </p>
        )}
      </div>

      <footer className="text-xs text-zinc-500">
        {!runningInTauri && "Modo navegador (sem núcleo Rust)"}
        {info &&
          `${info.name} v${info.version} · protocolo v${info.protocolVersion}`}
      </footer>
    </main>
  );
}
