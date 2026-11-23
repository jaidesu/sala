import { useState } from "react";

interface HostTestProps {
  invite: string;
  /** Convidados conectados agora (identificadores curtos). */
  guests: string[];
  /** Registro de entradas e saídas, mais recente primeiro. */
  log: string[];
  onStop: () => void;
}

/**
 * Tela do Host na fase 2 (teste de conexão): mostra o convite e quem está
 * conectado. Na fase 5 vira a tela da sala de verdade.
 */
export default function HostTest({ invite, guests, log, onStop }: HostTestProps) {
  const [copied, setCopied] = useState(false);

  async function copy() {
    try {
      await navigator.clipboard.writeText(invite);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // Sem acesso à área de transferência: o usuário pode selecionar o texto.
      setCopied(false);
    }
  }

  return (
    <main className="mx-auto flex h-full max-w-2xl flex-col gap-6 p-6">
      <header className="flex items-center justify-between">
        <h1 className="text-2xl font-semibold">Sala aberta</h1>
        <button
          type="button"
          onClick={onStop}
          className="rounded-lg border border-red-800 px-3 py-2 text-sm text-red-300 hover:bg-red-950"
        >
          Encerrar sala
        </button>
      </header>

      <section className="flex flex-col gap-2">
        <h2 className="text-sm font-medium text-zinc-400">Convite</h2>
        <textarea
          readOnly
          value={invite}
          onFocus={(e) => e.currentTarget.select()}
          rows={4}
          className="w-full resize-none break-all rounded-lg border border-zinc-800 bg-zinc-900 p-3 font-mono text-xs text-zinc-300"
        />
        <div className="flex items-center gap-3">
          <button
            type="button"
            onClick={copy}
            className="rounded-lg bg-emerald-600 px-4 py-2 text-sm font-medium text-white hover:bg-emerald-500"
          >
            {copied ? "Copiado!" : "Copiar convite"}
          </button>
          <p className="text-xs text-zinc-500">
            Mantenha o app aberto e o computador ligado enquanto a sala existir.
          </p>
        </div>
      </section>

      <section className="flex flex-col gap-2">
        <h2 className="text-sm font-medium text-zinc-400">
          Conectados ({guests.length})
        </h2>
        {guests.length === 0 ? (
          <p className="text-sm text-zinc-500">Ninguém ainda.</p>
        ) : (
          <ul className="flex flex-wrap gap-2">
            {guests.map((g) => (
              <li
                key={g}
                className="rounded-full bg-zinc-800 px-3 py-1 font-mono text-xs"
              >
                {g}
              </li>
            ))}
          </ul>
        )}
      </section>

      <section className="flex min-h-0 flex-1 flex-col gap-2">
        <h2 className="text-sm font-medium text-zinc-400">Atividade</h2>
        <ul className="min-h-0 flex-1 overflow-y-auto text-sm text-zinc-400">
          {log.map((line, i) => (
            <li key={i}>{line}</li>
          ))}
        </ul>
      </section>
    </main>
  );
}
