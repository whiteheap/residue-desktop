import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";
import { kumoSources, phosphorWeights } from "./bundle.ts";

// Tauri serves the dev server on a fixed port and loads the build from dist/.
// It listens on 127.0.0.1 alone, the address devUrl in tauri.conf.json names,
// since a debug build gives that page the app's commands: never a name such as
// localhost, which also means ::1.
// bundle.ts keeps three of Phosphor's six icon weights, and fails the build
// on a weight it drops or a Kumo component the stylesheet does not scan.
export default defineConfig({
  plugins: [react(), tailwindcss(), phosphorWeights(), kumoSources()],
  clearScreen: false,
  server: {
    host: "127.0.0.1",
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  envPrefix: ["VITE_", "TAURI_ENV_"],
  build: {
    // macOS 13 is the oldest system the app supports; its WebKit is Safari 16.
    target: "safari16",
    sourcemap: false,
  },
});
