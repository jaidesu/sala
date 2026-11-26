import type { Participant } from "../lib/tauri";

/** Painel com quem está na sala agora. */
export default function ParticipantList({
  participants,
  myId,
}: {
  participants: Participant[];
  myId: number;
}) {
  return (
    <aside className="hidden w-52 shrink-0 flex-col gap-2 border-l border-zinc-800 p-3 sm:flex">
      <h2 className="text-xs font-semibold uppercase tracking-wide text-zinc-500">
        Na sala ({participants.length})
      </h2>
      <ul className="flex flex-col gap-1 overflow-y-auto text-sm">
        {participants.map((p) => (
          <li key={p.id} className="flex items-center gap-2 truncate">
            <span className="h-2 w-2 shrink-0 rounded-full bg-emerald-400" title="online" />
            <span className="truncate">{p.nickname}</span>
            {p.isHost && <span className="text-[10px] text-zinc-500">host</span>}
            {p.id === myId && <span className="text-[10px] text-zinc-500">você</span>}
          </li>
        ))}
      </ul>
    </aside>
  );
}
