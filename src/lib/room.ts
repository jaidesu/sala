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

export function applyEvent(view: RoomView, event: NetEvent): RoomView {
  switch (event.kind) {
    case "chat":
      // Evita duplicar se a mesma mensagem chegar duas vezes.
      if (view.timeline.some((i) => i.kind === "chat" && i.message.id === event.message.id)) {
        return view;
      }
      return { ...view, timeline: [...view.timeline, chatItem(event.message)] };
    case "participantJoined":
      return {
        ...view,
        participants: [...view.participants, event.participant],
        timeline: [...view.timeline, system(`${event.participant.nickname} entrou.`)],
      };
    case "participantLeft":
      return {
        ...view,
        participants: view.participants.filter((p) => p.id !== event.participant.id),
        timeline: [...view.timeline, system(`${event.participant.nickname} saiu.`)],
      };
    case "rejected":
      return { ...view, timeline: [...view.timeline, system(event.reason, "warn")] };
    case "joinRefused":
      return {
        ...view,
        timeline: [...view.timeline, system("Tentativa de entrada recusada.", "warn")],
      };
    case "pathChanged":
      return view.ended ? view : { ...view, connection: event.path };
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
        timeline: [...view.timeline, system("A conexão com o Host caiu.", "warn")],
      };
  }
}
