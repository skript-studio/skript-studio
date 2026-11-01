/**
 * Docs store — SkriptHub entries cache, search query, selected entry.
 *
 * Entries are fetched either from the Tauri backend (which proxies the
 * cached SkriptHub response from the LSP) or directly from the public
 * API in browser dev mode.
 */

import { create } from "zustand";
import type { SkriptHubEntry, SkriptSyntaxType } from "@/types/skript";

interface DocsStore {
  entries: SkriptHubEntry[];
  loading: boolean;
  error: string | null;
  query: string;
  /** Active type filter — empty set means "all types". */
  typeFilter: Set<SkriptSyntaxType>;
  selectedId: number | null;

  load: () => Promise<void>;
  setQuery: (q: string) => void;
  toggleType: (t: SkriptSyntaxType) => void;
  clearTypes: () => void;
  select: (id: number | null) => void;
}

export const useDocsStore = create<DocsStore>((set, get) => ({
  entries: [],
  loading: false,
  error: null,
  query: "",
  typeFilter: new Set(),
  selectedId: null,

  load: async () => {
    if (get().loading) return;
    set({ loading: true, error: null });
    try {
      const isTauri =
        typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
      let data: SkriptHubEntry[];
      if (isTauri) {
        const { invoke } = await import("@tauri-apps/api/core");
        data = await invoke<SkriptHubEntry[]>("get_skript_docs");
      } else {
        // Browser dev mode: fetch from the public API directly.
        const resp = await fetch(
          "https://skripthub.net/api/v1/addonsyntaxlist/",
        );
        if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
        data = (await resp.json()) as SkriptHubEntry[];
      }
      set({ entries: data, loading: false });
    } catch (err) {
      set({
        loading: false,
        error: err instanceof Error ? err.message : String(err),
      });
    }
  },

  setQuery: (q) => set({ query: q }),
  toggleType: (t) =>
    set((s) => {
      const next = new Set(s.typeFilter);
      if (next.has(t)) next.delete(t);
      else next.add(t);
      return { typeFilter: next };
    }),
  clearTypes: () => set({ typeFilter: new Set() }),
  select: (id) => set({ selectedId: id }),
}));
