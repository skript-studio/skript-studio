/**
 * LSP store — connection status, port, log buffer, diagnostics map.
 *
 * The actual WebSocket / MonacoLanguageClient lifecycle is managed in
 * the `use-lsp` hook; this store only holds observable state that
 * other components (status bar, output panel, future problems panel)
 * need to read.
 *
 * Diagnostics are keyed by file path so the editor tabs can show
 * per-file issue counts and the future Problems panel can group by
 * file. The Monaco editor itself owns its own marker model; this
 * store is for the surrounding chrome.
 */

import { create } from "zustand";
import type { Diagnostic, LSPLogEntry, LSPStatus } from "@/types/lsp";

interface LSPStore {
  status: LSPStatus;
  port: number | null;
  error: string | null;
  logs: LSPLogEntry[];
  /** Diagnostics per file path. */
  diagnosticsByFile: Map<string, Diagnostic[]>;

  setStatus: (status: LSPStatus, error?: string | null) => void;
  setPort: (port: number | null) => void;
  addLog: (entry: LSPLogEntry) => void;
  clearLogs: () => void;

  setDiagnostics: (path: string, diagnostics: Diagnostic[]) => void;
  clearDiagnostics: (path: string) => void;
  /** Total issue count across all open files. */
  totalIssues: () => number;
}

const MAX_LOGS = 500;

export const useLSPStore = create<LSPStore>((set, get) => ({
  status: "disconnected",
  port: null,
  error: null,
  logs: [],
  diagnosticsByFile: new Map(),

  setStatus: (status, error = null) => set({ status, error }),
  setPort: (port) => set({ port }),

  addLog: (entry) =>
    set((state) => ({
      logs: [...state.logs.slice(-MAX_LOGS + 1), entry],
    })),

  clearLogs: () => set({ logs: [] }),

  setDiagnostics: (path, diagnostics) =>
    set((state) => {
      const next = new Map(state.diagnosticsByFile);
      if (diagnostics.length === 0) {
        next.delete(path);
      } else {
        next.set(path, diagnostics);
      }
      return { diagnosticsByFile: next };
    }),

  clearDiagnostics: (path) =>
    set((state) => {
      const next = new Map(state.diagnosticsByFile);
      next.delete(path);
      return { diagnosticsByFile: next };
    }),

  totalIssues: () => {
    let total = 0;
    for (const diags of get().diagnosticsByFile.values()) {
      total += diags.length;
    }
    return total;
  },
}));
