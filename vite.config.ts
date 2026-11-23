import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
// @ts-expect-error type error without @types/node package
import process from "node:process";
const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(() => ({
  plugins: [react(), tailwindcss()],

  // Opções do Vite ajustadas para o Tauri (valem em `tauri dev` e `tauri build`)
  //
  // 1. não limpar a tela, para não esconder erros do Rust
  clearScreen: false,
  // 2. o Tauri espera uma porta fixa; falha se ela estiver ocupada
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. não observar a pasta `src-tauri` (o Tauri cuida dela)
      ignored: ["**/src-tauri/**"],
    },
  },
}));
