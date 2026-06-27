/**
 * Tauri LSP lifecycle hook. Wraps `start_lsp`, `stop_lsp`, `restart_lsp`,
 * `get_lsp_status` commands and keeps `useLSPStore` in sync.
 *
 * The `start` and `restart` methods accept `LspSpawnOptions` which the
 * backend forwards both to the sidecar's CLI flags (--cache-dir,
 * --log-level, --skripthub-url) and to the LSP server's
 * `initializationOptions`.
 */

import { useCallback, useEffect } from "react";
import {
  getLSPStatus,
  isTauri,
  restartLSP,
  startLSP,
  stopLSP,
  type LspSpawnOptions,
} from "@/lib/tauri-commands";
import { useLSPStore } from "@/stores/lsp-store";

export interface UseTauriLSPResult {
  available: boolean;
  start: (options?: LspSpawnOptions) => Promise<void>;
  stop: () => Promise<void>;
  restart: () => Promise<void>;
  refreshStatus: () => Promise<void>;
}

export function useTauriLSP(): UseTauriLSPResult {
  const available = isTauri();
  const setStatus = useLSPStore((s) => s.setStatus);
  const setPort = useLSPStore((s) => s.setPort);

  const refreshStatus = useCallback(async () => {
    if (!available) return;
    try {
      const result = await getLSPStatus();
      setStatus(result.status, result.error);
      setPort(result.port ?? null);
    } catch (err) {
      setStatus("error", err instanceof Error ? err.message : String(err));
    }
  }, [available, setStatus, setPort]);

  const start = useCallback(
    async (options?: LspSpawnOptions) => {
      if (!available) {
        setStatus("error", "Tauri backend unavailable");
        return;
      }
      setStatus("starting");
      try {
        const result = await startLSP(options);
        setPort(result.port);
        setStatus("connected");
      } catch (err) {
        setStatus("error", err instanceof Error ? err.message : String(err));
      }
    },
    [available, setStatus, setPort],
  );

  const stop = useCallback(async () => {
    if (!available) return;
    try {
      await stopLSP();
      setPort(null);
      setStatus("disconnected");
    } catch (err) {
      setStatus("error", err instanceof Error ? err.message : String(err));
    }
  }, [available, setStatus, setPort]);

  const restart = useCallback(async () => {
    if (!available) return;
    setStatus("starting");
    try {
      const result = await restartLSP();
      setPort(result.port);
      setStatus("connected");
    } catch (err) {
      setStatus("error", err instanceof Error ? err.message : String(err));
    }
  }, [available, setStatus, setPort]);

  // Poll status every 5s while mounted. The LSP server does NOT
  // self-recover from panics — it exits non-zero — so the frontend
  // owns the restart responsibility (per backend contract #12).
  useEffect(() => {
    if (!available) return;
    void refreshStatus();
    const id = setInterval(refreshStatus, 5000);
    return () => clearInterval(id);
  }, [available, refreshStatus]);

  return { available, start, stop, restart, refreshStatus };
}
