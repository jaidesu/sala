import { MIN_PASSWORD_CHARS, strength, type Strength } from "../lib/password";

const LEVELS: Record<Strength, { label: string; bar: string; width: string }> = {
  tooShort: {
    label: `Muito curta (mínimo ${MIN_PASSWORD_CHARS} caracteres)`,
    bar: "bg-red-600",
    width: "w-1/12",
  },
  weak: { label: "Fraca: fácil de adivinhar", bar: "bg-red-500", width: "w-1/3" },
  fair: { label: "Razoável", bar: "bg-amber-400", width: "w-2/3" },
  strong: { label: "Forte", bar: "bg-emerald-500", width: "w-full" },
};

/** Barra colorida com a força estimada da senha. */
export default function PasswordMeter({ password }: { password: string }) {
  if (!password) return null;
  const level = LEVELS[strength(password)];
  return (
    <div className="flex flex-col gap-1" aria-live="polite">
      <div className="h-1.5 w-full overflow-hidden rounded-full bg-zinc-800">
        <div className={`h-full rounded-full ${level.bar} ${level.width}`} />
      </div>
      <p className="text-xs text-zinc-400">{level.label}</p>
    </div>
  );
}
