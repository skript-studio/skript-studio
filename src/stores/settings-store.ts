/**
 * Settings store. Persists via Tauri's app-data directory; in browser dev
 * mode (no Tauri) it falls back to localStorage.
 */

import { create } from "zustand";
import {
  type AppSettings,
  DEFAULT_SETTINGS,
} from "@/types/settings";

interface SettingsStore extends AppSettings {
  loaded: boolean;
  /** Hydrate from disk/localStorage. */
  load: () => Promise<void>;
  /** Persist the current settings. */
  save: () => Promise<void>;
  /** Patch the appearance settings. */
  updateAppearance: (patch: Partial<AppSettings["appearance"]>) => void;
  /** Patch the LSP settings. */
  updateLSP: (patch: Partial<AppSettings["lsp"]>) => void;
  /** Reset everything to defaults. */
  reset: () => void;
}

const STORAGE_KEY = "skript-studio.settings";

/** Detect whether we're running inside Tauri or a plain browser. */
function isTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

async function readFromDisk(): Promise<AppSettings | null> {
  if (isTauri()) {
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const raw = await invoke<string | null>("read_settings");
      return raw ? (JSON.parse(raw) as AppSettings) : null;
    } catch (err) {
      console.warn("Failed to read settings via Tauri:", err);
      return null;
    }
  }
  // Browser fallback
  const raw = localStorage.getItem(STORAGE_KEY);
  return raw ? (JSON.parse(raw) as AppSettings) : null;
}

async function writeToDisk(settings: AppSettings): Promise<void> {
  if (isTauri()) {
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("write_settings", { settings: JSON.stringify(settings) });
      return;
    } catch (err) {
      console.warn("Failed to write settings via Tauri:", err);
    }
  }
  localStorage.setItem(STORAGE_KEY, JSON.stringify(settings));
}

export const useSettingsStore = create<SettingsStore>((set, get) => ({
  ...DEFAULT_SETTINGS,
  loaded: false,

  load: async () => {
    const stored = await readFromDisk();
    if (stored) {
      set({ ...stored, loaded: true });
    } else {
      set({ ...DEFAULT_SETTINGS, loaded: true });
    }
  },

  save: async () => {
    const { loaded, load, save, updateAppearance, updateLSP, reset, ...rest } =
      get();
    await writeToDisk(rest as AppSettings);
  },

  updateAppearance: (patch) => {
    set((s) => ({ appearance: { ...s.appearance, ...patch } }));
    void get().save();
  },

  updateLSP: (patch) => {
    set((s) => ({ lsp: { ...s.lsp, ...patch } }));
    void get().save();
  },

  reset: () => {
    set({ ...DEFAULT_SETTINGS });
    void get().save();
  },
}));
