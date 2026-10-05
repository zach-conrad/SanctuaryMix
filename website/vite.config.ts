import { resolve } from "node:path";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Static pages: the landing page, the account page (sign-in), a recorded service
// in that account, and the public page a share link opens.
// Relative base so the build works at a domain root or under a sub-path
// (for example GitHub Pages at /SanctuaryMix/).
export default defineConfig({
  base: "./",
  plugins: [react()],
  // The pages reuse the desktop app's pure recording helpers in ../src/lib.
  server: { fs: { allow: [".."] } },
  build: {
    rollupOptions: {
      input: {
        home: resolve(__dirname, "index.html"),
        account: resolve(__dirname, "account/index.html"),
        service: resolve(__dirname, "account/service/index.html"),
        share: resolve(__dirname, "share/index.html"),
      },
    },
  },
});
