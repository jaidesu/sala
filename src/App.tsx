import { useState } from "react";
import { useRoom } from "./hooks/useRoom";
import { demoRoom, isDemo } from "./lib/demo";
import type { NetMode } from "./lib/tauri";
import CreateRoom from "./screens/CreateRoom";
import Home from "./screens/Home";
import JoinRoom from "./screens/JoinRoom";
import Room from "./screens/Room";

/** Qual formulário está aberto quando ainda não estamos numa sala. */
type Screen =
  | { kind: "home" }
  | { kind: "create"; mode: NetMode }
  | { kind: "join"; mode: NetMode };

/**
 * Raiz da interface: escolhe a tela. Se há uma sala ativa, mostra a sala;
 * senão, a tela inicial ou um dos formulários.
 */
export default function App() {
  const [screen, setScreen] = useState<Screen>({ kind: "home" });
  const { room, busy, error, create, join, send, kick, exit, clearError } = useRoom();

  // Modo demonstração (só no navegador, com `?demo`): mostra uma sala de
  // exemplo para ajustar o visual sem compilar o Rust.
  if (isDemo) {
    return (
      <Room view={demoRoom} onSend={async () => true} onExit={() => {}} onKick={() => {}} />
    );
  }

  if (room) {
    return (
      <Room
        view={room}
        onSend={send}
        onKick={kick}
        onExit={async () => {
          await exit();
          setScreen({ kind: "home" });
        }}
      />
    );
  }

  function goHome() {
    clearError();
    setScreen({ kind: "home" });
  }

  switch (screen.kind) {
    case "home":
      return (
        <Home
          onCreate={(mode) => {
            clearError();
            setScreen({ kind: "create", mode });
          }}
          onJoin={(mode) => {
            clearError();
            setScreen({ kind: "join", mode });
          }}
        />
      );
    case "create":
      return (
        <CreateRoom
          busy={busy}
          error={error}
          onCreate={(password, nickname) => create(screen.mode, password, nickname)}
          onBack={goHome}
        />
      );
    case "join":
      return (
        <JoinRoom
          busy={busy}
          error={error}
          onJoin={(invite, password, nickname) =>
            join(screen.mode, invite, password, nickname)
          }
          onBack={goHome}
        />
      );
  }
}
