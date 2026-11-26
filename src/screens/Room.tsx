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
  /** Host: remover um participante. */
  onKick?: (id: number) => void;
}

/** Tela da sala: mensagens, campo de envio e participantes. */
export default function Room({ view, onSend, onExit, onKick }: RoomProps) {
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

  const others = view.participants.filter((p) => p.id !== view.me.id).length;
  const emptyHint = isHost
    ? others === 0
      ? "Ninguém entrou ainda. Copie o convite e mande a senha por outro canal."
      : "Ninguém disse nada ainda. Que tal começar?"
    : "Ninguém disse nada ainda. Que tal começar?";

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
          <span
            title="Os convidados conectam direto ao seu computador."
            className="inline-flex items-center gap-2 rounded-full border border-zinc-800 bg-zinc-900 px-3 py-1 text-sm"
          >
            <span className="h-2 w-2 rounded-full bg-emerald-400" />
            Sala online · {others} {others === 1 ? "convidado" : "convidados"}
          </span>
        )}
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

      {isHost && !view.ended && (
        <p className="border-b border-amber-900/50 bg-amber-950/40 px-4 py-1.5 text-xs text-amber-200">
          A sala vive no seu computador: mantenha o app aberto e não deixe o computador
          suspender (desligue a suspensão automática enquanto a sala estiver aberta).
        </p>
      )}
      {view.ended && (
        <p className="border-b border-zinc-800 bg-zinc-900 px-4 py-1.5 text-xs text-zinc-400">
          A sala acabou. O histórico existia só na memória do Host e foi descartado.
        </p>
      )}

      <div className="flex min-h-0 flex-1">
        <section className="flex min-w-0 flex-1 flex-col">
          <MessageList items={view.timeline} myId={view.me.id} emptyHint={emptyHint} />
          <MessageInput
            disabled={view.ended || view.connection === "reconnecting"}
            placeholder={
              view.ended
                ? "Sala encerrada"
                : view.connection === "reconnecting"
                  ? "Reconectando… aguarde"
                  : undefined
            }
            onSend={onSend}
          />
        </section>
        <ParticipantList
          participants={view.participants}
          myId={view.me.id}
          onKick={isHost && !view.ended ? onKick : undefined}
        />
      </div>
    </main>
  );
}
