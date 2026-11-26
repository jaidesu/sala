/**
 * Estado da sala na interface e como cada evento de rede o modifica.
 *
 * `applyEvent` é uma função pura (recebe o estado e devolve um novo), no
 * estilo "reducer" do React: fica fácil de entender e de testar.
 */
import type { ConnectionState } from "../components/ConnectionBadge";
import type { ChatMessage, NetEvent, Participant, RoomSnapshot } from "./tauri";

/** Um item da linha do tempo: mensagem de alguém ou aviso do sistema. */
export type TimelineItem =
  | { kind: "chat"; key: string; message: ChatMessage }
  | { kind: "system"; key: string; text: string; tone: "info" | "warn" };

export interface RoomView {
  role: "host" | "guest";
  /** Só para o Host: o convite a compartilhar. */
  invite?: string;
  me: Participant;
  participants: Participant[];
  timeline: TimelineItem[];
  /** Só para o Convidado. */
  connection?: ConnectionState;
  /** A sala acabou (encerrada pelo Host ou conexão perdida). */
  ended: boolean;
}

let systemCounter = 0;
function system(text: string, tone: "info" | "warn" = "info"): TimelineItem {
  systemCounter += 1;
  return { kind: "system", key: `s${systemCounter}`, text, tone };
}

function chatItem(message: ChatMessage): TimelineItem {
  return { kind: "chat", key: `m${message.id}`, message };
}

export function fromSnapshot(
  role: "host" | "guest",
  room: RoomSnapshot,
  extra: Partial<RoomView> = {},
): RoomView {
  return {
    role,
    me: room.me,
    participants: room.participants,
    timeline: [
      ...room.history.map(chatItem),
      system(role === "host" ? "Sala aberta." : `Você entrou como ${room.me.nickname}.`),
    ],
    ended: false,
    ...extra,
  };
}

/** Substitui (pelo id) ou acrescenta um participante na lista. */
function upsert(list: Participant[], p: Participant): Participant[] {
  return list.some((x) => x.id === p.id)
    ? list.map((x) => (x.id === p.id ? p : x))
    : [...list, p];
}

/** Acrescenta à linha do tempo as mensagens que ainda não estão nela. */
function mergeHistory(timeline: TimelineItem[], history: ChatMessage[]): TimelineItem[] {
  const known = new Set(
    timeline.flatMap((i) => (i.kind === "chat" ? [i.message.id] : [])),
  );
  return [...timeline, ...history.filter((m) => !known.has(m.id)).map(chatItem)];
}

export function applyEvent(view: RoomView, event: NetEvent): RoomView {
  switch (event.kind) {
    case "chat":
      return { ...view, timeline: mergeHistory(view.timeline, [event.message]) };
    case "participantJoined":
      return {
        ...view,
        participants: upsert(view.participants, event.participant),
        timeline: [...view.timeline, system(`${event.participant.nickname} entrou.`)],
      };
    case "participantUpdated": {
      const p = event.participant;
      return {
        ...view,
        participants: upsert(view.participants, p),
        timeline: [
          ...view.timeline,
          system(p.online ? `${p.nickname} voltou.` : `${p.nickname} perdeu a conexão.`),
        ],
      };
    }
    case "participantLeft":
      return {
        ...view,
        participants: view.participants.filter((p) => p.id !== event.participant.id),
        timeline: [
          ...view.timeline,
          system(
            event.kicked
              ? `${event.participant.nickname} foi removido(a) pelo Host.`
              : `${event.participant.nickname} saiu.`,
          ),
        ],
      };
    case "reconnecting":
      if (view.ended || view.connection === "reconnecting") return view;
      return {
        ...view,
        connection: "reconnecting",
        timeline: [
          ...view.timeline,
          system("Conexão perdida. Tentando reconectar…", "warn"),
        ],
      };
    case "resynced":
      return {
        ...view,
        me: event.room.me,
        participants: event.room.participants,
        connection: event.path,
        timeline: [
          ...mergeHistory(view.timeline, event.room.history),
          system("Reconectado."),
        ],
      };
    case "kicked":
      return {
        ...view,
        ended: true,
        connection: "disconnected",
        timeline: [...view.timeline, system("Você foi removido(a) da sala pelo Host.", "warn")],
      };
    case "rejected":
      return { ...view, timeline: [...view.timeline, system(event.reason, "warn")] };
    case "joinRefused":
      return {
        ...view,
        timeline: [...view.timeline, system("Tentativa de entrada recusada.", "warn")],
      };
    case "pathChanged":
      // Durante a reconexão, avisos de caminho da conexão antiga não valem.
      return view.ended || view.connection === "reconnecting"
        ? view
        : { ...view, connection: event.path };
    case "roomClosed":
      return {
        ...view,
        ended: true,
        connection: "disconnected",
        timeline: [...view.timeline, system("O Host encerrou a sala.", "warn")],
      };
    case "disconnected":
      if (view.ended) return view;
      return {
        ...view,
        ended: true,
        connection: "disconnected",
        timeline: [
          ...view.timeline,
          system(
            "Não foi possível reconectar ao Host. A sala pode ter sido encerrada ou o computador dele desligou.",
            "warn",
          ),
        ],
      };
  }
}
