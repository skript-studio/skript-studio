/**
 * Editor store — open files, active tab, dirty tracking, content buffer.
 */

import { create } from "zustand";
import { basename } from "@/lib/utils";

export interface OpenFile {
  path: string;
  content: string;
  isDirty: boolean;
  /** Line/column to restore when switching tabs. */
  scrollTop?: number;
  cursorLine?: number;
  cursorColumn?: number;
}

interface EditorStore {
  projectRoot: string | null;
  openFiles: OpenFile[];
  activeTab: string | null;

  openFile: (path: string, content: string) => void;
  closeTab: (path: string) => void;
  setActiveTab: (path: string) => void;
  updateContent: (path: string, content: string) => void;
  markDirty: (path: string) => void;
  markClean: (path: string) => void;
  setProjectRoot: (path: string) => void;
  saveCursor: (path: string, line: number, column: number) => void;
  hasUnsavedChanges: () => boolean;
}

export const useEditorStore = create<EditorStore>((set, get) => ({
  projectRoot: null,
  openFiles: [],
  activeTab: null,

  openFile: (path, content) =>
    set((state) => {
      const existing = state.openFiles.find((f) => f.path === path);
      if (existing) {
        return { activeTab: path };
      }
      return {
        openFiles: [
          ...state.openFiles,
          { path, content, isDirty: false },
        ],
        activeTab: path,
      };
    }),

  closeTab: (path) =>
    set((state) => {
      const idx = state.openFiles.findIndex((f) => f.path === path);
      if (idx === -1) return state;

      const next = state.openFiles.filter((f) => f.path !== path);
      let active = state.activeTab;
      if (state.activeTab === path) {
        if (next.length === 0) active = null;
        else if (idx >= next.length) active = next[next.length - 1].path;
        else active = next[idx].path;
      }
      return { openFiles: next, activeTab: active };
    }),

  setActiveTab: (path) => set({ activeTab: path }),

  updateContent: (path, content) =>
    set((state) => ({
      openFiles: state.openFiles.map((f) =>
        f.path === path
          ? { ...f, content, isDirty: f.content !== content ? true : f.isDirty }
          : f,
      ),
    })),

  markDirty: (path) =>
    set((state) => ({
      openFiles: state.openFiles.map((f) =>
        f.path === path ? { ...f, isDirty: true } : f,
      ),
    })),

  markClean: (path) =>
    set((state) => ({
      openFiles: state.openFiles.map((f) =>
        f.path === path ? { ...f, isDirty: false } : f,
      ),
    })),

  setProjectRoot: (path) => set({ projectRoot: path }),

  saveCursor: (path, line, column) =>
    set((state) => ({
      openFiles: state.openFiles.map((f) =>
        f.path === path
          ? { ...f, cursorLine: line, cursorColumn: column }
          : f,
      ),
    })),

  hasUnsavedChanges: () =>
    get().openFiles.some((f) => f.isDirty),
}));

/** Convenience selector for the active file object. */
export function selectActiveFile(state: EditorStore): OpenFile | null {
  if (!state.activeTab) return null;
  return state.openFiles.find((f) => f.path === state.activeTab) ?? null;
}

/** Display name for a tab — just the basename. */
export function tabDisplayName(path: string): string {
  return basename(path);
}
