/**
 * Selo com o estado da conexão, como pede o SPEC.md:
 * conectando / direta / via relay / desconectado.
 */
export type ConnectionState =
  | "connecting"
  | "direct"
  | "relay"
  | "unknown"
  | "disconnected";

const STYLES: Record<ConnectionState, { label: string; dot: string; hint: string }> = {
  connecting: {
    label: "Conectando…",
    dot: "bg-amber-400 animate-pulse",
    hint: "Procurando o Host.",
  },
  direct: {
    label: "Conexão direta",
    dot: "bg-emerald-400",
    hint: "Os dados vão direto entre os dois computadores.",
  },
  relay: {
    label: "Via relay",
    dot: "bg-sky-400",
    hint: "Os dados passam por um servidor relay, cifrados: ele não lê o conteúdo.",
  },
  unknown: {
    label: "Conectado",
    dot: "bg-zinc-400",
    hint: "Conectado; o caminho ainda está sendo escolhido.",
  },
  disconnected: {
    label: "Desconectado",
    dot: "bg-red-500",
    hint: "Sem conexão com o Host.",
  },
};

export default function ConnectionBadge({ state }: { state: ConnectionState }) {
  const s = STYLES[state];
  return (
    <span
      title={s.hint}
      className="inline-flex items-center gap-2 rounded-full border border-zinc-800 bg-zinc-900 px-3 py-1 text-sm"
    >
      <span className={`h-2 w-2 rounded-full ${s.dot}`} />
      {s.label}
    </span>
  );
}
