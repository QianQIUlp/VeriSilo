import { defineConfig, loadEnv } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig(({ mode }) => ({
  plugins: [react()],
  clearScreen: false,
  // Engine packages and Tauri-generated HTML are build artifacts, not app entries.
  optimizeDeps: { entries: ["index.html", "preview.html"] },
  server: {
    host: "127.0.0.1",
    port: Number(loadEnv(mode, ".", "VERISILO_DEV_").VERISILO_DEV_PORT ?? 1420),
    strictPort: true,
    watch: { ignored: ["**/src-tauri/target/**"] },
  },
  build: {
    target: "es2022",
    sourcemap: true,
  },
}));
