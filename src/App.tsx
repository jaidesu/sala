import { useEffect, useState } from "react";
import type { ConnectionState } from "./components/ConnectionBadge";
import {
  errorMessage,
  guestJoin,
  guestLeave,
  hostStart,
  hostStop,
  onNetEvent,
  runningInTauri,
  type NetMode,
  type PathKind,
} from "./lib/tauri";
import GuestTest from "./screens/GuestTest";
import Home from "./screens/Home";
import HostTest from "./screens/HostTest";

/** Qual tela está aberta. */
type Screen =
  | { kind: "home" }
  | { kind: "host"; invite: string }
  | { kind: "guest"; mode: NetMode };

function pathToState(path: PathKind): ConnectionState {
  return path;
}

function timestamp() {
  return new Date().toLocaleTimeString();
}

/**
 * Raiz da interface: controla qual tela aparece e guarda o estado vindo dos
 * eventos de rede do núcleo Rust.
 */
export default function App() {
  const [screen, setScreen] = useState<Screen>({ kind: "home" });
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Estado do Host
  const [guests, setGuests] = useState<string[]>([]);
  const [log, setLog] = useState<string[]>([]);

  // Estado do Convidado
  const [connection, setConnection] = useState<ConnectionState | null>(null);

  // Escuta os eventos de rede durante toda a vida do app.
  useEffect(() => {
    if (!runningInTauri) return;
    let unlisten: (() => void) | undefined;
    let cancelled = false;

    onNetEvent((event) => {
      switch (event.kind) {
        case "guestConnected":
          setGuests((g) => [...g, event.guest]);
          setLog((l) => [`${timestamp()} · ${event.guest} entrou`, ...l]);
          break;
        case "guestDisconnected":
          setGuests((g) => g.filter((x) => x !== event.guest));
          setLog((l) => [`${timestamp()} · ${event.guest} saiu`, ...l]);
          break;
        case "pathChanged":
          setConnection(pathToState(event.path));
          break;
        case "disconnected":
          setConnection("disconnected");
          break;
      }
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

  async function createRoom(mode: NetMode) {
    setBusy(true);
    setError(null);
    try {
      const invite = await hostStart(mode);
      setGuests([]);
      setLog([`${timestamp()} · sala aberta`]);
      setScreen({ kind: "host", invite });
    } catch (err) {
      setError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  }

  async function stopRoom() {
    await hostStop().catch(() => {});
    setScreen({ kind: "home" });
  }

  async function joinRoom(invite: string) {
    if (screen.kind !== "guest") return;
    setError(null);
    setConnection("connecting");
    try {
      const path = await guestJoin(invite, screen.mode);
      setConnection(pathToState(path));
    } catch (err) {
      setConnection(null);
      setError(errorMessage(err));
    }
  }

  async function leaveRoom() {
    await guestLeave().catch(() => {});
    setConnection(null);
    setError(null);
    setScreen({ kind: "home" });
  }

  switch (screen.kind) {
    case "home":
      return (
        <Home
          error={error}
          busy={busy}
          onCreate={createRoom}
          onJoin={(mode) => {
            setError(null);
            setConnection(null);
            setScreen({ kind: "guest", mode });
          }}
        />
      );
    case "host":
      return (
        <HostTest invite={screen.invite} guests={guests} log={log} onStop={stopRoom} />
      );
    case "guest":
      return (
        <GuestTest
          connection={connection}
          error={error}
          onConnect={joinRoom}
          onLeave={leaveRoom}
        />
      );
  }
}
