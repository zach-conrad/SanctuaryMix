import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri expects a fixed port and must not have Vite clear its logs.
const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: { ignored: ["**/src-tauri/**", "**/crates/**", "**/target/**"] },
  },
  build: {
    // Safari 15 is the WebKit floor on macOS 12, our minimum.
    target: ["es2021", "safari15"],
    sourcemap: !!process.env.TAURI_ENV_DEBUG,
  },
});
