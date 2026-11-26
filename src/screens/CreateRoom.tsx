import { useState } from "react";
import PasswordMeter from "../components/PasswordMeter";
import { MIN_PASSWORD_CHARS, strength } from "../lib/password";
import { errorMessage, generatePassword, MAX_NICKNAME_CHARS } from "../lib/tauri";

interface CreateRoomProps {
  busy: boolean;
  error: string | null;
  onCreate: (password: string, nickname: string) => void;
  onBack: () => void;
}

/**
 * Tela "Criar sala": escolher (ou gerar) a senha antes de abrir a sala.
 * A senha não vai no convite: o Host combina com os Convidados por outro canal.
 */
export default function CreateRoom({ busy, error, onCreate, onBack }: CreateRoomProps) {
  const [nickname, setNickname] = useState("");
  const [password, setPassword] = useState("");
  const [visible, setVisible] = useState(false);
  const [copied, setCopied] = useState(false);
  const [genError, setGenError] = useState<string | null>(null);

  const level = strength(password);
  const tooShort = Array.from(password).length < MIN_PASSWORD_CHARS;
  const ready = !tooShort && nickname.trim() !== "" && !busy;

  async function copyPassword() {
    try {
      await navigator.clipboard.writeText(password);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      setCopied(false);
    }
  }

  async function generate() {
    setGenError(null);
    try {
      setPassword(await generatePassword());
      setVisible(true); // senha gerada precisa ser vista para ser repassada
    } catch (err) {
      setGenError(errorMessage(err));
    }
  }

  return (
    <main className="mx-auto flex h-full max-w-md flex-col justify-center gap-6 p-6">
      <header>
        <h1 className="text-2xl font-semibold">Criar sala</h1>
        <p className="mt-1 text-sm text-zinc-400">
          Quem quiser entrar vai precisar do convite <strong>e</strong> desta senha.
        </p>
      </header>

      <form
        className="flex flex-col gap-3"
        onSubmit={(e) => {
          e.preventDefault();
          if (ready) onCreate(password, nickname);
        }}
      >
        <label htmlFor="host-nickname" className="text-sm font-medium text-zinc-400">
          Seu apelido
        </label>
        <input
          id="host-nickname"
          value={nickname}
          onChange={(e) => setNickname(e.target.value)}
          maxLength={MAX_NICKNAME_CHARS}
          className="rounded-lg border border-zinc-800 bg-zinc-900 px-3 py-2 text-sm"
        />

        <label htmlFor="password" className="text-sm font-medium text-zinc-400">
          Senha da sala
        </label>
        <div className="flex gap-2">
          <input
            id="password"
            type={visible ? "text" : "password"}
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            autoComplete="new-password"
            spellCheck={false}
            className="min-w-0 flex-1 rounded-lg border border-zinc-800 bg-zinc-900 px-3 py-2 font-mono text-sm"
          />
          <button
            type="button"
            onClick={() => setVisible((v) => !v)}
            className="rounded-lg border border-zinc-700 px-3 text-sm hover:bg-zinc-900"
          >
            {visible ? "Ocultar" : "Mostrar"}
          </button>
        </div>

        <PasswordMeter password={password} />

        <div className="flex gap-4">
          <button
            type="button"
            onClick={generate}
            className="text-sm text-emerald-400 hover:underline"
          >
            Gerar senha forte
          </button>
          {password && (
            <button
              type="button"
              onClick={copyPassword}
              className="text-sm text-emerald-400 hover:underline"
            >
              {copied ? "Senha copiada!" : "Copiar senha"}
            </button>
          )}
        </div>
        <p className="text-xs text-zinc-500">
          Guarde a senha agora: você vai passá-la aos convidados por um canal diferente
          do convite.
        </p>
        {genError && <p className="text-sm text-red-300">{genError}</p>}

        {level === "weak" && (
          <p className="rounded-md bg-amber-950 px-3 py-2 text-sm text-amber-200">
            Senha fraca. O app limita as tentativas, mas uma senha fácil ainda pode
            ser adivinhada. Prefira a senha gerada.
          </p>
        )}

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
            {busy ? "Preparando a sala…" : "Criar sala"}
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
