import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { TimelineItem } from "../lib/room";

function time(ms: number) {
  return new Date(ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

/** Distância (px) do fim da lista em que ainda consideramos "lá embaixo". */
const NEAR_BOTTOM = 80;

/**
 * Lista de mensagens.
 *
 * Segurança: o texto é sempre renderizado como texto puro (`{message.text}`),
 * nunca como HTML. Assim, uma mensagem com `<script>` ou `<img onerror=…>`
 * aparece literalmente na tela, sem ser executada.
 *
 * Rolagem: se você está no fim da conversa, novas mensagens rolam a tela
 * sozinhas. Se subiu para ler algo antigo, a tela fica parada e aparece um
 * botão "novas mensagens ↓".
 */
export default function MessageList({
  items,
  myId,
  emptyHint,
}: {
  items: TimelineItem[];
  myId: number;
  /** Texto mostrado enquanto ninguém mandou mensagem. */
  emptyHint: string;
}) {
  const box = useRef<HTMLDivElement>(null);
  const atBottom = useRef(true);
  const [unseen, setUnseen] = useState(0);
  const lastCount = useRef(items.length);

  function scrollToBottom() {
    const el = box.current;
    if (el) el.scrollTop = el.scrollHeight;
    atBottom.current = true;
    setUnseen(0);
  }

  // Ao chegar algo novo: rola se já estávamos no fim; senão, conta como não visto.
  useLayoutEffect(() => {
    const added = items.length - lastCount.current;
    lastCount.current = items.length;
    if (added <= 0) return;
    const newChats = items
      .slice(-added)
      .flatMap((i) => (i.kind === "chat" ? [i.message] : []));
    // Mensagem minha sempre rola (acabei de enviar, quero ver).
    const sentByMe = newChats.some((m) => m.from === myId);
    if (atBottom.current || sentByMe) scrollToBottom();
    else if (newChats.length > 0) setUnseen((n) => n + newChats.length);
  }, [items, myId]);

  // Começa mostrando o fim da conversa.
  useEffect(() => {
    scrollToBottom();
  }, []);

  function onScroll() {
    const el = box.current;
    if (!el) return;
    atBottom.current = el.scrollHeight - el.scrollTop - el.clientHeight < NEAR_BOTTOM;
    if (atBottom.current) setUnseen(0);
  }

  const hasChat = items.some((i) => i.kind === "chat");

  return (
    <div className="relative flex min-h-0 flex-1 flex-col">
      <div
        ref={box}
        onScroll={onScroll}
        className="flex min-h-0 flex-1 flex-col gap-2 overflow-y-auto px-4 py-3"
        role="log"
        aria-live="polite"
      >
        {!hasChat && (
          <p className="my-auto text-center text-sm text-zinc-500">{emptyHint}</p>
        )}
        {items.map((item) => {
          if (item.kind === "system") {
            return (
              <p
                key={item.key}
                className={`text-center text-xs ${
                  item.tone === "warn" ? "text-amber-300" : "text-zinc-500"
                }`}
              >
                {item.text}
              </p>
            );
          }
          const m = item.message;
          const mine = m.from === myId;
          return (
            <div key={item.key} className={`flex ${mine ? "justify-end" : "justify-start"}`}>
              <div
                className={`max-w-[75%] rounded-2xl px-3 py-2 ${
                  mine ? "bg-emerald-700 text-white" : "bg-zinc-800"
                }`}
              >
                {!mine && (
                  <p className="text-xs font-semibold text-emerald-300">{m.nickname}</p>
                )}
                <p className="whitespace-pre-wrap break-words text-sm">{m.text}</p>
                <p
                  className={`mt-0.5 text-right text-[10px] ${
                    mine ? "text-emerald-200" : "text-zinc-500"
                  }`}
                >
                  {time(m.timestamp)}
                </p>
              </div>
            </div>
          );
        })}
      </div>

      {unseen > 0 && (
        <button
          type="button"
          onClick={scrollToBottom}
          className="absolute bottom-3 left-1/2 -translate-x-1/2 rounded-full bg-emerald-600 px-3 py-1 text-xs font-medium text-white shadow-lg hover:bg-emerald-500"
        >
          {unseen === 1 ? "1 nova mensagem" : `${unseen} novas mensagens`} ↓
        </button>
      )}
    </div>
  );
}
