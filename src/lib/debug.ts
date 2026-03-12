/**
 * Debug snapshot — DEV builds only.
 *
 * Exposes `window.__spatia_debug_snapshot()` which serializes the full
 * Zustand store and writes the result to disk via the `write_debug_snapshot`
 * Tauri command.  The function also returns the JSON string so callers can
 * inspect it directly in devtools or from a shell script.
 *
 * Gated behind `import.meta.env.DEV` so the symbol never exists in
 * production bundles.
 */

import { useAppStore } from "./appStore";
import { isTauri } from "./tauri";
import { invoke } from "@tauri-apps/api/core";

export function installDebugSnapshot(): void {
  if (!import.meta.env.DEV) {
    return;
  }

  (window as unknown as Record<string, unknown>).__spatia_debug_snapshot =
    async (): Promise<string> => {
      const state = useAppStore.getState();

      const snapshot = {
        timestamp: new Date().toISOString(),
        tables: state.tables,
        chatMessages: state.chatMessages,
        isProcessing: state.isProcessing,
        analysisGeoJson: state.analysisGeoJson,
        mapActions: state.mapActions,
      };

      const json = JSON.stringify(snapshot, null, 2);

      // Persist to disk when running inside Tauri so shell scripts can read it.
      if (isTauri()) {
        try {
          await invoke("write_debug_snapshot", { data: json });
        } catch (err) {
          console.warn("[spatia] write_debug_snapshot failed:", err);
        }
      }

      return json;
    };
}
