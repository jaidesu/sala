import { useEffect, useRef } from "react";
import type { TimelineItem } from "../lib/room";

function time(ms: number) {
  return new Date(ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

/**
 * Lista de mensagens. Segurança: o texto é sempre renderizado como texto puro
 * (`{message.text}`), nunca como HTML. Assim, uma mensagem com `<script>` ou
 * `<img onerror=…>` aparece literalmente na tela, sem ser executada.
 */
export default function MessageList({
  items,
  myId,
}: {
  items: TimelineItem[];
  myId: number;
}) {
  const bottom = useRef<HTMLDivElement>(null);

  // Rola até a última mensagem quando chega algo novo.
  useEffect(() => {
    bottom.current?.scrollIntoView({ block: "end" });
  }, [items.length]);

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-2 overflow-y-auto px-4 py-3">
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
              <p className={`mt-0.5 text-right text-[10px] ${mine ? "text-emerald-200" : "text-zinc-500"}`}>
                {time(m.timestamp)}
              </p>
            </div>
          </div>
        );
      })}
      <div ref={bottom} />
    </div>
  );
}
