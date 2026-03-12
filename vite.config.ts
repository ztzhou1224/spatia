import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import path from "path";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [react(), tailwindcss()],

  resolve: {
    alias: {
      "@": path.resolve("src"),
      // @loaders.gl/worker-utils imports `spawn` from child_process inside its
      // child-process-proxy.js. That code path is only used in Node/worker
      // environments; it is never reached in a browser build. We stub the module
      // with an empty shim so Vite/Rollup does not emit the
      // "'spawn' is not exported by __vite-browser-external" warning.
      child_process: path.resolve("src/shims/child_process.js"),
    },
  },

  build: {
    rollupOptions: {
      output: {
        manualChunks: {
          maplibre: ["maplibre-gl"],
          deckgl: [
            "@deck.gl/core",
            "@deck.gl/layers",
            "@deck.gl/aggregation-layers",
            "@deck.gl/mapbox",
          ],
          recharts: ["recharts"],
        },
      },
    },
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
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
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
