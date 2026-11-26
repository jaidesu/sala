import { useState } from "react";
import { MAX_MESSAGE_CHARS } from "../lib/tauri";

/**
 * Campo de mensagem. Enter envia; Shift+Enter quebra a linha.
 * O texto só é apagado se o envio der certo.
 */
export default function MessageInput({
  disabled,
  placeholder,
  onSend,
}: {
  disabled: boolean;
  /** Texto do campo vazio (padrão: "Escreva uma mensagem…"). */
  placeholder?: string;
  onSend: (text: string) => Promise<boolean>;
}) {
  const [text, setText] = useState("");
  const [sending, setSending] = useState(false);

  async function submit() {
    if (!text.trim() || sending || disabled) return;
    setSending(true);
    const ok = await onSend(text);
    setSending(false);
    if (ok) setText("");
  }

  return (
    <form
      className="flex items-end gap-2 border-t border-zinc-800 p-3"
      onSubmit={(e) => {
        e.preventDefault();
        void submit();
      }}
    >
      <textarea
        value={text}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          // `isComposing` evita enviar no meio da digitação de acentos (IME).
          if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
            e.preventDefault();
            void submit();
          }
        }}
        disabled={disabled}
        autoFocus
        maxLength={MAX_MESSAGE_CHARS}
        rows={1}
        placeholder={placeholder ?? "Escreva uma mensagem…"}
        className="max-h-40 min-h-10 flex-1 resize-none rounded-lg border border-zinc-800 bg-zinc-900 px-3 py-2 text-sm disabled:opacity-50"
      />
      <button
        type="submit"
        disabled={disabled || sending || !text.trim()}
        className="rounded-lg bg-emerald-600 px-4 py-2 text-sm font-medium text-white hover:bg-emerald-500 disabled:cursor-not-allowed disabled:opacity-50"
      >
        Enviar
      </button>
    </form>
  );
}
