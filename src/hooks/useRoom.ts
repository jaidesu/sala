/**
 * `useRoom`: tudo o que a interface precisa para estar numa sala.
 *
 * Junta num lugar só:
 * - a escuta dos eventos de rede vindos do núcleo Rust;
 * - o estado da sala (`RoomView`), atualizado por `applyEvent`;
 * - as ações: criar, entrar, enviar e sair.
 *
 * Os componentes de tela só recebem o estado pronto e chamam as ações.
 */
import { useEffect, useRef, useState } from "react";
import { applyEvent, fromSnapshot, type RoomView } from "../lib/room";
import {
  errorMessage,
  guestJoin,
  guestLeave,
  hostStart,
  hostStop,
  kickParticipant,
  onNetEvent,
  runningInTauri,
  sendMessage,
  type NetEvent,
  type NetMode,
} from "../lib/tauri";

export function useRoom() {
  const [room, setRoom] = useState<RoomView | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Eventos que chegam enquanto a sala está sendo criada/aberta (antes de
  // termos a foto inicial) ficam guardados aqui e são aplicados em seguida.
  const pending = useRef<NetEvent[] | null>(null);

  useEffect(() => {
    if (!runningInTauri) return;
    let unlisten: (() => void) | undefined;
    let cancelled = false;

    onNetEvent((event) => {
      if (pending.current) {
        pending.current.push(event);
        return;
      }
      setRoom((r) => (r ? applyEvent(r, event) : r));
    }).then((fn) => {
      // Se o componente já foi desmontado, para de escutar na hora.
      if (cancelled) fn();
      else unlisten = fn;
    });

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  /** Mostra a sala a partir da foto inicial + eventos que chegaram no meio. */
  function enter(view: RoomView) {
    const queued = pending.current ?? [];
    pending.current = null;
    setRoom(queued.reduce(applyEvent, view));
  }

  /** Roda `action` marcando "ocupado" e guardando o erro, se houver. */
  async function attempt(action: () => Promise<RoomView>): Promise<boolean> {
    setBusy(true);
    setError(null);
    pending.current = [];
    try {
      enter(await action());
      return true;
    } catch (err) {
      pending.current = null;
      setError(errorMessage(err));
      return false;
    } finally {
      setBusy(false);
    }
  }

  function create(mode: NetMode, password: string, nickname: string) {
    return attempt(async () => {
      const started = await hostStart(mode, password, nickname);
      return fromSnapshot("host", started.room, { invite: started.invite });
    });
  }

  function join(mode: NetMode, invite: string, password: string, nickname: string) {
    return attempt(async () => {
      const joined = await guestJoin(invite, password, nickname, mode);
      return fromSnapshot("guest", joined.room, { connection: joined.path });
    });
  }

  async function send(text: string): Promise<boolean> {
    try {
      await sendMessage(text);
      return true;
    } catch (err) {
      setRoom((r) =>
        r ? applyEvent(r, { kind: "rejected", reason: errorMessage(err) }) : r,
      );
      return false;
    }
  }

  /** (Host) Remove alguém; se der errado, mostra o motivo na conversa. */
  async function kick(id: number) {
    try {
      await kickParticipant(id);
    } catch (err) {
      setRoom((r) =>
        r ? applyEvent(r, { kind: "rejected", reason: errorMessage(err) }) : r,
      );
    }
  }

  async function exit() {
    if (room?.role === "host") await hostStop().catch(() => {});
    else await guestLeave().catch(() => {});
    setRoom(null);
    setError(null);
  }

  function clearError() {
    setError(null);
  }

  return { room, busy, error, create, join, send, kick, exit, clearError };
}
