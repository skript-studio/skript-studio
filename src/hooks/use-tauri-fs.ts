/**
 * Tauri filesystem hook. Wraps the `tauri-commands` module in React-friendly
 * async functions with error handling, and exposes a `mockFallback` mode
 * for browser dev (where Tauri is unavailable).
 */

import { useCallback, useState } from "react";
import {
  createFile as cmdCreateFile,
  deleteFile as cmdDeleteFile,
  isTauri,
  listDir as cmdListDir,
  openFolder as cmdOpenFolder,
  readFile as cmdReadFile,
  renameFile as cmdRenameFile,
  writeFile as cmdWriteFile,
  type DirEntry,
} from "@/lib/tauri-commands";

export interface UseTauriFsResult {
  /** True when running inside Tauri (commands are available). */
  available: boolean;
  /** Last error message, if any. */
  error: string | null;
  clearError: () => void;

  openFolder: () => Promise<string | null>;
  listDir: (path: string) => Promise<DirEntry[]>;
  readFile: (path: string) => Promise<string>;
  writeFile: (path: string, content: string) => Promise<void>;
  createFile: (path: string) => Promise<void>;
  deleteFile: (path: string) => Promise<void>;
  renameFile: (oldPath: string, newPath: string) => Promise<void>;
}

/**
 * Hook for Tauri FS operations. When running outside Tauri (browser dev)
 * every command throws a friendly error that the caller can catch and
 * surface as a toast / inline message.
 */
export function useTauriFs(): UseTauriFsResult {
  const [error, setError] = useState<string | null>(null);

  const wrap = useCallback(
    <A extends unknown[], R>(
      fn: (...args: A) => Promise<R>,
    ) => async (...args: A): Promise<R> => {
      try {
        const result = await fn(...args);
        setError(null);
        return result;
      } catch (err) {
        const msg = err instanceof Error ? err.message : String(err);
        setError(msg);
        throw err;
      }
    },
    [],
  );

  return {
    available: isTauri(),
    error,
    clearError: () => setError(null),
    openFolder: wrap(cmdOpenFolder),
    listDir: wrap(cmdListDir),
    readFile: wrap(cmdReadFile),
    writeFile: wrap(cmdWriteFile),
    createFile: wrap(cmdCreateFile),
    deleteFile: wrap(cmdDeleteFile),
    renameFile: wrap(cmdRenameFile),
  };
}
