import { useState } from "react";
import type { Participant } from "../lib/tauri";

/**
 * Painel com quem está na sala: verde = online; cinza = conexão caída
 * (a pessoa tem até 1 minuto para voltar sozinha antes de sair da sala).
 *
 * Para o Host, cada Convidado tem um botão "remover" que pede confirmação
 * (segundo clique).
 */
export default function ParticipantList({
  participants,
  myId,
  onKick,
}: {
  participants: Participant[];
  myId: number;
  /** Só para o Host. */
  onKick?: (id: number) => void;
}) {
  const [confirming, setConfirming] = useState<number | null>(null);

  function kick(id: number) {
    if (confirming === id) {
      setConfirming(null);
      onKick?.(id);
      return;
    }
    setConfirming(id);
    setTimeout(() => setConfirming((c) => (c === id ? null : c)), 4000);
  }

  const online = participants.filter((p) => p.online).length;

  return (
    <aside className="hidden w-56 shrink-0 flex-col gap-2 border-l border-zinc-800 p-3 sm:flex">
      <h2 className="text-xs font-semibold uppercase tracking-wide text-zinc-500">
        Na sala ({online}/{participants.length})
      </h2>
      <ul className="flex flex-col gap-1 overflow-y-auto text-sm">
        {participants.map((p) => (
          <li key={p.id} className="group flex items-center gap-2">
            <span
              className={`h-2 w-2 shrink-0 rounded-full ${
                p.online ? "bg-emerald-400" : "bg-zinc-600"
              }`}
              title={p.online ? "online" : "offline: conexão caiu, pode voltar"}
            />
            <span className={`truncate ${p.online ? "" : "text-zinc-500"}`}>{p.nickname}</span>
            {p.isHost && <span className="text-[10px] text-zinc-500">host</span>}
            {p.id === myId && <span className="text-[10px] text-zinc-500">você</span>}
            {!p.online && <span className="text-[10px] text-zinc-600">offline</span>}
            {onKick && !p.isHost && p.id !== myId && (
              <button
                type="button"
                onClick={() => kick(p.id)}
                className={`ml-auto shrink-0 rounded px-1.5 text-[10px] ${
                  confirming === p.id
                    ? "bg-red-900 text-red-100"
                    : "text-zinc-500 opacity-0 hover:text-red-300 focus:opacity-100 group-hover:opacity-100"
                }`}
              >
                {confirming === p.id ? "confirmar" : "remover"}
              </button>
            )}
          </li>
        ))}
      </ul>
    </aside>
  );
}
