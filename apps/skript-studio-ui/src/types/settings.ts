/**
 * Settings schema. Persisted by the Tauri backend to the OS app-data
 * directory as JSON.
 */

export type ThemeMode = "dark" | "light" | "system";

export interface AppearanceSettings {
  theme: ThemeMode;
  fontSize: number;
  fontFamily: string;
  tabSize: 2 | 4 | 8;
  minimap: boolean;
  wordWrap: boolean;
}

export interface LSPSettings {
  /** Override the default SkriptHub API URL. */
  skripthubApiUrl: string;
  /** LSP log verbosity. */
  logLevel: "error" | "warn" | "info" | "debug" | "trace";
  /** Auto-restart the LSP sidecar if it crashes. */
  autoRestart: boolean;
}

export interface AppSettings {
  appearance: AppearanceSettings;
  lsp: LSPSettings;
}

export const DEFAULT_SETTINGS: AppSettings = {
  appearance: {
    theme: "dark",
    fontSize: 14,
    fontFamily: "'JetBrains Mono', 'Fira Code', Menlo, monospace",
    tabSize: 4,
    minimap: true,
    wordWrap: false,
  },
  lsp: {
    skripthubApiUrl: "https://skripthub.net/api/v1/addonsyntaxlist/",
    logLevel: "info",
    autoRestart: true,
  },
};

export const FONT_FAMILY_OPTIONS = [
  { label: "JetBrains Mono", value: "'JetBrains Mono', 'Fira Code', Menlo, monospace" },
  { label: "Fira Code", value: "'Fira Code', Menlo, monospace" },
  { label: "Menlo", value: "Menlo, Monaco, monospace" },
  { label: "Consolas", value: "Consolas, monospace" },
  { label: "Source Code Pro", value: "'Source Code Pro', monospace" },
];
