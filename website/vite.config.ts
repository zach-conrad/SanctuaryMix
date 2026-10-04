import { resolve } from "node:path";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Two static pages: the landing page and the sample account page.
// Relative base so the build works at a domain root or under a sub-path
// (for example GitHub Pages at /SanctuaryMix/).
export default defineConfig({
  base: "./",
  plugins: [react()],
  build: {
    rollupOptions: {
      input: {
        home: resolve(__dirname, "index.html"),
        account: resolve(__dirname, "account/index.html"),
      },
    },
  },
});
