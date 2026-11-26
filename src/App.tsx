import { useEffect, useRef, useState } from "react";
import { applyEvent, fromSnapshot, type RoomView } from "./lib/room";
import {
  errorMessage,
  guestJoin,
  guestLeave,
  hostStart,
  hostStop,
  onNetEvent,
  runningInTauri,
  sendMessage,
  type NetEvent,
  type NetMode,
} from "./lib/tauri";
import CreateRoom from "./screens/CreateRoom";
import Home from "./screens/Home";
import JoinRoom from "./screens/JoinRoom";
import Room from "./screens/Room";

/** Qual tela está aberta (a sala em si fica em `room`). */
type Screen =
  | { kind: "home" }
  | { kind: "create"; mode: NetMode }
  | { kind: "join"; mode: NetMode }
  | { kind: "room" };

/**
 * Raiz da interface: controla qual tela aparece e aplica os eventos de rede
 * do núcleo Rust ao estado da sala.
 */
export default function App() {
  const [screen, setScreen] = useState<Screen>({ kind: "home" });
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
  function enterRoom(view: RoomView) {
    const queued = pending.current ?? [];
    pending.current = null;
    setRoom(queued.reduce(applyEvent, view));
    setScreen({ kind: "room" });
  }

  function goHome() {
    setError(null);
    setRoom(null);
    setScreen({ kind: "home" });
  }

  async function createRoom(password: string, nickname: string) {
    if (screen.kind !== "create") return;
    setBusy(true);
    setError(null);
    pending.current = [];
    try {
      const started = await hostStart(screen.mode, password, nickname);
      enterRoom(fromSnapshot("host", started.room, { invite: started.invite }));
    } catch (err) {
      pending.current = null;
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  }

  async function joinRoom(invite: string, password: string, nickname: string) {
    if (screen.kind !== "join") return;
    setBusy(true);
    setError(null);
    pending.current = [];
    try {
      const joined = await guestJoin(invite, password, nickname, screen.mode);
      enterRoom(fromSnapshot("guest", joined.room, { connection: joined.path }));
    } catch (err) {
      pending.current = null;
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
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

  async function exitRoom() {
    if (room?.role === "host") await hostStop().catch(() => {});
    else await guestLeave().catch(() => {});
    goHome();
  }

  switch (screen.kind) {
    case "home":
      return (
        <Home
          onCreate={(mode) => {
            setError(null);
            setScreen({ kind: "create", mode });
          }}
          onJoin={(mode) => {
            setError(null);
            setScreen({ kind: "join", mode });
          }}
        />
      );
    case "create":
      return (
        <CreateRoom busy={busy} error={error} onCreate={createRoom} onBack={goHome} />
      );
    case "join":
      return <JoinRoom busy={busy} error={error} onJoin={joinRoom} onBack={goHome} />;
    case "room":
      return room ? <Room view={room} onSend={send} onExit={exitRoom} /> : null;
  }
}
