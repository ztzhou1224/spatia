import { invoke as tauriInvoke } from "@tauri-apps/api/core";

/** True when running inside the Tauri desktop shell. */
export const isTauri = (): boolean =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/**
 * Wraps Tauri's invoke() with graceful degradation for browser-only dev mode.
 *
 * When the Tauri backend is unavailable (e.g., `npm run dev` without Tauri),
 * this returns `fallback` instead of throwing so that the UI still renders
 * with mock data.
 *
 * BLOCKER: Every call site that falls back to `fallback` must be wired to a
 *   real invoke("execute_engine_command", { command }) once Tauri is running.
 */
export async function safeInvoke<T>(
  cmd: string,
  args?: Record<string, unknown>,
  fallback?: T,
): Promise<T | undefined> {
  if (!isTauri()) {
    return fallback;
  }
  try {
    return await tauriInvoke<T>(cmd, args);
  } catch (err) {
    console.warn(`[spatia] invoke("${cmd}") failed:`, err);
    return fallback;
  }
}
