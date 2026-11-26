import { useState } from "react";
import ConnectionBadge from "../components/ConnectionBadge";
import MessageInput from "../components/MessageInput";
import MessageList from "../components/MessageList";
import ParticipantList from "../components/ParticipantList";
import type { RoomView } from "../lib/room";

interface RoomProps {
  view: RoomView;
  onSend: (text: string) => Promise<boolean>;
  /** Host: encerrar a sala. Convidado: sair. Depois do fim: voltar ao início. */
  onExit: () => void;
}

/** Tela da sala: mensagens, campo de envio e participantes. */
export default function Room({ view, onSend, onExit }: RoomProps) {
  const [copied, setCopied] = useState(false);
  // Sair/encerrar pede um segundo clique (confirmação dentro da própria tela;
  // `window.confirm` não funciona em todos os WebViews).
  const [confirming, setConfirming] = useState(false);
  const isHost = view.role === "host";

  function exit() {
    if (view.ended || confirming) {
      onExit();
      return;
    }
    setConfirming(true);
    setTimeout(() => setConfirming(false), 4000);
  }

  async function copyInvite() {
    if (!view.invite) return;
    try {
      await navigator.clipboard.writeText(view.invite);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      setCopied(false);
    }
  }

  const exitLabel = view.ended
    ? "Voltar ao início"
    : confirming
      ? isHost
        ? "Confirmar: encerrar para todos"
        : "Confirmar saída"
      : isHost
        ? "Encerrar sala"
        : "Sair";

  return (
    <main className="flex h-full flex-col">
      <header className="flex items-center gap-3 border-b border-zinc-800 px-4 py-3">
        <h1 className="text-lg font-semibold">Sala</h1>
        {view.connection && <ConnectionBadge state={view.connection} />}
        {isHost && (
          <button
            type="button"
            onClick={copyInvite}
            title="Mande o convite por um canal e a senha por outro."
            className="rounded-lg border border-zinc-700 px-3 py-1 text-sm hover:bg-zinc-900"
          >
            {copied ? "Convite copiado!" : "Copiar convite"}
          </button>
        )}
        <button
          type="button"
          onClick={exit}
          className={`ml-auto rounded-lg px-3 py-1 text-sm ${
            isHost && !view.ended
              ? "border border-red-800 text-red-300 hover:bg-red-950"
              : "border border-zinc-700 hover:bg-zinc-900"
          }`}
        >
          {exitLabel}
        </button>
      </header>

      {isHost && (
        <p className="border-b border-zinc-900 bg-zinc-900/50 px-4 py-1.5 text-xs text-zinc-500">
          Mantenha o app aberto e o computador ligado: a sala só existe enquanto você estiver aqui.
        </p>
      )}

      <div className="flex min-h-0 flex-1">
        <section className="flex min-w-0 flex-1 flex-col">
          <MessageList items={view.timeline} myId={view.me.id} />
          <MessageInput disabled={view.ended} onSend={onSend} />
        </section>
        <ParticipantList participants={view.participants} myId={view.me.id} />
      </div>
    </main>
  );
}
