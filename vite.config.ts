import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri erwartet einen festen Port und keine Bildschirm-Löschung.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 1420, strictPort: true, watch: { ignored: ["**/src-tauri/**", "**/target/**"] } },
  envPrefix: ["VITE_", "TAURI_ENV_*"],
  build: { target: "es2022", sourcemap: false },
});
