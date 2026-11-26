import { useState } from "react";
import { MAX_NICKNAME_CHARS } from "../lib/tauri";

interface JoinRoomProps {
  busy: boolean;
  error: string | null;
  onJoin: (invite: string, password: string, nickname: string) => void;
  onBack: () => void;
}

/** Tela "Entrar em uma sala": convite, senha e apelido. */
export default function JoinRoom({ busy, error, onJoin, onBack }: JoinRoomProps) {
  const [invite, setInvite] = useState("");
  const [password, setPassword] = useState("");
  const [nickname, setNickname] = useState("");

  const ready = invite.trim() && password && nickname.trim() && !busy;

  return (
    <main className="mx-auto flex h-full max-w-md flex-col justify-center gap-6 p-6">
      <header>
        <h1 className="text-2xl font-semibold">Entrar em uma sala</h1>
        <p className="mt-1 text-sm text-zinc-400">
          Cole o convite e digite a senha que o Host passou para você.
        </p>
      </header>

      <form
        className="flex flex-col gap-3"
        onSubmit={(e) => {
          e.preventDefault();
          if (!ready) return;
          onJoin(invite, password, nickname);
          // Não guardamos a senha na tela depois de usá-la.
          setPassword("");
        }}
      >
        <label htmlFor="invite" className="text-sm font-medium text-zinc-400">
          Convite
        </label>
        <textarea
          id="invite"
          value={invite}
          onChange={(e) => setInvite(e.target.value)}
          placeholder="sala1:…"
          rows={3}
          spellCheck={false}
          className="w-full resize-none break-all rounded-lg border border-zinc-800 bg-zinc-900 p-3 font-mono text-xs"
        />

        <label htmlFor="join-password" className="text-sm font-medium text-zinc-400">
          Senha
        </label>
        <input
          id="join-password"
          type="password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          autoComplete="off"
          spellCheck={false}
          className="rounded-lg border border-zinc-800 bg-zinc-900 px-3 py-2 font-mono text-sm"
        />

        <label htmlFor="join-nickname" className="text-sm font-medium text-zinc-400">
          Seu apelido
        </label>
        <input
          id="join-nickname"
          value={nickname}
          onChange={(e) => setNickname(e.target.value)}
          maxLength={MAX_NICKNAME_CHARS}
          className="rounded-lg border border-zinc-800 bg-zinc-900 px-3 py-2 text-sm"
        />

        {error && (
          <p role="alert" className="rounded-md bg-red-950 px-3 py-2 text-sm text-red-300">
            {error}
          </p>
        )}

        <div className="mt-2 flex gap-3">
          <button
            type="submit"
            disabled={!ready}
            className="flex-1 rounded-lg bg-emerald-600 px-4 py-3 font-medium text-white hover:bg-emerald-500 disabled:cursor-not-allowed disabled:opacity-50"
          >
            {busy ? "Entrando…" : "Entrar"}
          </button>
          <button
            type="button"
            onClick={onBack}
            disabled={busy}
            className="rounded-lg border border-zinc-700 px-4 py-3 hover:bg-zinc-900 disabled:opacity-50"
          >
            Voltar
          </button>
        </div>
      </form>
    </main>
  );
}
