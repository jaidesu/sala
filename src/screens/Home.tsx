import { useEffect, useState } from "react";
import {
  getAppInfo,
  runningInTauri,
  type AppInfo,
  type NetMode,
} from "../lib/tauri";

interface HomeProps {
  onCreate: (mode: NetMode) => void;
  onJoin: (mode: NetMode) => void;
}

/**
 * Tela inicial: escolher entre criar uma sala ou entrar em uma.
 *
 * A opção "só rede local" desliga relays e descoberta pela internet. Serve
 * para testar em dois computadores na mesma rede, mesmo sem internet.
 */
export default function Home({ onCreate, onJoin }: HomeProps) {
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [localOnly, setLocalOnly] = useState(false);
  const mode: NetMode = localOnly ? "localOnly" : "internet";
  const disabled = !runningInTauri;

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
          Criar sala
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
      </div>

      <details className="w-full max-w-md text-sm text-zinc-400">
        <summary className="cursor-pointer text-center text-zinc-500 hover:text-zinc-300">
          Como sua privacidade é protegida?
        </summary>
        <ul className="mt-3 list-disc space-y-1 pl-5 text-xs leading-relaxed">
          <li>
            As mensagens vão cifradas de ponta a ponta e ficam só na memória do computador
            de quem criou a sala. Nada é salvo em servidor.
          </li>
          <li>
            A senha nunca trafega pela rede. Quem tem só o convite não entra.
          </li>
          <li>
            Quando a conexão direta não é possível, os dados passam por um servidor{" "}
            <em>relay</em>. Ele vê <strong>que</strong> há tráfego entre dois computadores,
            mas não <strong>o quê</strong>.
          </li>
          <li>
            Quem cria a sala vê todas as mensagens, e qualquer participante pode copiá-las.
          </li>
        </ul>
      </details>

      <footer className="text-xs text-zinc-500">
        {!runningInTauri && "Modo navegador (sem núcleo Rust)"}
        {info &&
          `${info.name} v${info.version} · protocolo v${info.protocolVersion}`}
      </footer>
    </main>
  );
}
