import { useState } from "react";
import ConnectionBadge, { type ConnectionState } from "../components/ConnectionBadge";
import { errorMessage, guestPing, type PingResult } from "../lib/tauri";

interface GuestTestProps {
  /** Estado da conexão; `null` = ainda não tentou conectar. */
  connection: ConnectionState | null;
  error: string | null;
  onConnect: (invite: string) => void;
  onLeave: () => void;
}

/**
 * Tela do Convidado na fase 2 (teste de conexão): colar o convite, conectar
 * e medir a latência com "ping". Na fase 5 vira a tela de entrar na sala.
 */
export default function GuestTest({ connection, error, onConnect, onLeave }: GuestTestProps) {
  const [invite, setInvite] = useState("");
  const [pings, setPings] = useState<PingResult[]>([]);
  const [pingError, setPingError] = useState<string | null>(null);

  const connected =
    connection === "direct" || connection === "relay" || connection === "unknown";

  async function ping() {
    setPingError(null);
    try {
      const result = await guestPing();
      setPings((p) => [result, ...p].slice(0, 10));
    } catch (err) {
      setPingError(errorMessage(err));
    }
  }

  return (
    <main className="mx-auto flex h-full max-w-2xl flex-col gap-6 p-6">
      <header className="flex items-center justify-between">
        <h1 className="text-2xl font-semibold">Entrar em uma sala</h1>
        <button
          type="button"
          onClick={onLeave}
          className="rounded-lg border border-zinc-700 px-3 py-2 text-sm hover:bg-zinc-900"
        >
          {connected ? "Sair" : "Voltar"}
        </button>
      </header>

      {!connected && (
        <form
          className="flex flex-col gap-3"
          onSubmit={(e) => {
            e.preventDefault();
            onConnect(invite);
          }}
        >
          <label className="text-sm font-medium text-zinc-400" htmlFor="invite">
            Convite
          </label>
          <textarea
            id="invite"
            value={invite}
            onChange={(e) => setInvite(e.target.value)}
            placeholder="sala1:…"
            rows={4}
            className="w-full resize-none break-all rounded-lg border border-zinc-800 bg-zinc-900 p-3 font-mono text-xs"
          />
          <div className="flex items-center gap-3">
            <button
              type="submit"
              disabled={!invite.trim() || connection === "connecting"}
              className="rounded-lg bg-emerald-600 px-4 py-2 text-sm font-medium text-white hover:bg-emerald-500 disabled:cursor-not-allowed disabled:opacity-50"
            >
              Conectar
            </button>
            {connection && <ConnectionBadge state={connection} />}
          </div>
        </form>
      )}

      {error && (
        <p role="alert" className="rounded-md bg-red-950 px-3 py-2 text-sm text-red-300">
          {error}
        </p>
      )}

      {connected && connection && (
        <section className="flex flex-col gap-4">
          <div className="flex items-center gap-3">
            <ConnectionBadge state={connection} />
            <button
              type="button"
              onClick={ping}
              className="rounded-lg bg-emerald-600 px-4 py-2 text-sm font-medium text-white hover:bg-emerald-500"
            >
              Ping
            </button>
          </div>
          {pingError && <p className="text-sm text-red-300">{pingError}</p>}
          <ul className="text-sm text-zinc-400">
            {pings.map((p, i) => (
              <li key={i}>
                {p.rttMs} ms · {p.path === "direct" ? "direto" : p.path === "relay" ? "via relay" : "caminho indefinido"}
              </li>
            ))}
          </ul>
        </section>
      )}
    </main>
  );
}
