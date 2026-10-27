import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { fileURLToPath } from "node:url";

const mockFile = (f: string) => fileURLToPath(new URL(`./mock/${f}`, import.meta.url));

export default defineConfig(({ mode }) => {
  // `npm run dev:ui` swaps the Tauri backend for demo data in ./mock,
  // so the interface can be developed and screenshotted in a browser.
  const mock = mode === "mock" || process.env.RL_MOCK === "1";
  return {
    plugins: [react()],
    clearScreen: false,
    server: { port: 5173, strictPort: true },
    build: { target: "es2022", sourcemap: false, chunkSizeWarningLimit: 800 },
    resolve: mock
      ? {
          alias: {
            "@tauri-apps/api/core": mockFile("core.ts"),
            "@tauri-apps/api/event": mockFile("event.ts"),
            "@tauri-apps/plugin-dialog": mockFile("plugins.ts"),
            "@tauri-apps/plugin-autostart": mockFile("plugins.ts"),
          },
        }
      : {},
  };
});
