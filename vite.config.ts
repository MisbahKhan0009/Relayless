import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { fileURLToPath } from "node:url";

const mock = process.env.RL_MOCK === "1";
const m = (f: string) => fileURLToPath(new URL(`./mock/${f}`, import.meta.url));

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 5173, strictPort: true },
  build: { target: "es2022", sourcemap: false, chunkSizeWarningLimit: 800 },
  // Browser preview with fake data (never used for the real app build).
  resolve: mock
    ? {
        alias: {
          "@tauri-apps/api/core": m("core.ts"),
          "@tauri-apps/api/event": m("event.ts"),
          "@tauri-apps/plugin-dialog": m("plugins.ts"),
          "@tauri-apps/plugin-autostart": m("plugins.ts"),
        },
      }
    : {},
});
