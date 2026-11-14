import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

export default defineConfig(({ command }) => ({
  plugins: [react()],
  clearScreen: false,
  server: { port: 5174, strictPort: true },
  // public/ only holds the browser test data (public/dev-fixture, your own games): never ship it
  publicDir: command === "serve" ? "public" : false,
  build: { target: "safari16", chunkSizeWarningLimit: 800 },
}));
