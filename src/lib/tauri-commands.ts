/**
 * Type-safe wrappers around Tauri `invoke()` calls.
 *
 * Every function detects whether we're running inside Tauri. In browser
 * dev mode (no `__TAURI_INTERNALS__`) they throw a friendly error so
 * the calling hook can fall back to mock behaviour.
 */

import { invoke as tauriInvoke } from "@tauri-apps/api/core";
import type {
  LSPStartResult,
  LSPStatusResult,
} from "@/types/lsp";

export interface DirEntry {
  name: string;
  is_dir: boolean;
  is_file: boolean;
  extension: string | null;
  /** Full path (parent + name) for convenience. */
  path: string;
}

/**
 * Spawn options forwarded to the LSP sidecar. Matches the Rust
 * `LspSpawnOptions` struct and the LSP server's `initializationOptions`
 * schema (camelCase keys).
 */
export interface LspSpawnOptions {
  /** Override the SkriptHub API URL. */
  skripthubUrl?: string;
  /** Log verbosity: trace | debug | info | warn | error. */
  logLevel?: "trace" | "debug" | "info" | "warn" | "error";
  /** Cap on completion items returned per request. */
  maxCompletions?: number;
}

export function isTauri(): boolean {
  return (
    typeof window !== "undefined" && "__TAURI_INTERNALS__" in window
  );
}

async function safeInvoke<T>(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<T> {
  if (!isTauri()) {
    throw new Error(
      `Tauri command "${cmd}" is not available — running outside the Tauri webview. ` +
        `Run this app via \`bun tauri dev\` to enable native filesystem and LSP access.`,
    );
  }
  return tauriInvoke<T>(cmd, args);
}

// --- File operations ----------------------------------------------------

export const openFolder = (): Promise<string | null> =>
  safeInvoke<string | null>("open_folder");

export const listDir = (path: string): Promise<DirEntry[]> =>
  safeInvoke<DirEntry[]>("list_dir", { path });

export const readFile = (path: string): Promise<string> =>
  safeInvoke<string>("read_file", { path });

export const writeFile = (path: string, content: string): Promise<void> =>
  safeInvoke<void>("write_file", { path, content });

export const createFile = (path: string): Promise<void> =>
  safeInvoke<void>("create_file", { path });

export const deleteFile = (path: string): Promise<void> =>
  safeInvoke<void>("delete_file", { path });

export const renameFile = (oldPath: string, newPath: string): Promise<void> =>
  safeInvoke<void>("rename_file", { oldPath, newPath });

// --- LSP operations -----------------------------------------------------

/**
 * Spawn the LSP sidecar. Pass `options` to forward SkriptHub URL /
 * log level / max completions to both the sidecar's CLI flags and
 * the LSP `initializationOptions`. These options are remembered by
 * the backend and re-applied on `restartLSP`.
 */
export const startLSP = (options?: LspSpawnOptions): Promise<LSPStartResult> =>
  safeInvoke<LSPStartResult>("start_lsp", { options: options ?? null });

export const stopLSP = (): Promise<void> => safeInvoke<void>("stop_lsp");

export const restartLSP = (): Promise<LSPStartResult> =>
  safeInvoke<LSPStartResult>("restart_lsp");

export const getLSPStatus = (): Promise<LSPStatusResult> =>
  safeInvoke<LSPStatusResult>("get_lsp_status");
