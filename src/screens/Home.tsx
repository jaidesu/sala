import { useEffect, useState } from "react";
import { getAppInfo, runningInTauri, type AppInfo } from "../lib/tauri";

/**
 * Tela inicial: escolher entre criar uma sala ou entrar em uma.
 *
 * Na fase 1 os botões ainda não fazem nada. O rodapé mostra a versão vinda
 * do núcleo Rust: se ela aparece, a ponte interface ↔ Rust está funcionando.
 */
export default function Home() {
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [error, setError] = useState(false);

  useEffect(() => {
    getAppInfo()
      .then(setInfo)
      .catch(() => setError(true));
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
          disabled
          className="rounded-lg bg-emerald-600 px-4 py-3 font-medium text-white disabled:cursor-not-allowed disabled:opacity-50"
        >
          Criar sala
        </button>
        <button
          type="button"
          disabled
          className="rounded-lg border border-zinc-700 px-4 py-3 font-medium disabled:cursor-not-allowed disabled:opacity-50"
        >
          Entrar em uma sala
        </button>
        <p className="text-center text-xs text-zinc-500">
          Disponível nas próximas fases.
        </p>
      </div>

      <footer className="text-xs text-zinc-500">
        {!runningInTauri && "Modo navegador (sem núcleo Rust)"}
        {runningInTauri && error && "Não foi possível falar com o núcleo"}
        {info &&
          `${info.name} v${info.version} · protocolo v${info.protocolVersion}`}
      </footer>
    </main>
  );
}
