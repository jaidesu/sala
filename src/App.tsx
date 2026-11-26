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
} from "./lib/tauri";
import CreateRoom from "./screens/CreateRoom";
import GuestTest from "./screens/GuestTest";
import Home from "./screens/Home";
import HostTest from "./screens/HostTest";

/** Qual tela está aberta. */
type Screen =
  | { kind: "home" }
  | { kind: "create"; mode: NetMode }
  | { kind: "host"; invite: string }
  | { kind: "guest"; mode: NetMode };

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
        case "joinRefused":
          setLog((l) => [`${timestamp()} · tentativa de entrada recusada`, ...l]);
          break;
        case "pathChanged":
          setConnection(event.path);
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

  function goHome() {
    setError(null);
    setScreen({ kind: "home" });
  }

  async function createRoom(password: string) {
    if (screen.kind !== "create") return;
    setBusy(true);
    setError(null);
    try {
      const invite = await hostStart(screen.mode, password);
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
    goHome();
  }

  async function joinRoom(invite: string, password: string) {
    if (screen.kind !== "guest") return;
    setError(null);
    setConnection("connecting");
    try {
      const path = await guestJoin(invite, password, screen.mode);
      setConnection(path);
    } catch (err) {
      setConnection(null);
      setError(errorMessage(err));
    }
  }

  async function leaveRoom() {
    await guestLeave().catch(() => {});
    setConnection(null);
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
            setConnection(null);
            setScreen({ kind: "guest", mode });
          }}
        />
      );
    case "create":
      return (
        <CreateRoom busy={busy} error={error} onCreate={createRoom} onBack={goHome} />
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
