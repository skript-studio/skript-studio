/**
 * Custom Monaco themes tuned to match the IDE shell's CSS variables
 * in both light and dark modes. Token colors mirror VS Code's dark+
 * palette so the local tokenizer and the LSP semantic tokens produce
 * a consistent look.
 */

import type { editor } from "monaco-editor";

const DARK_TOKEN_RULES: editor.ITokenThemeRule[] = [
  { token: "comment", foreground: "8b949e", fontStyle: "italic" },
  { token: "keyword", foreground: "ff7b72" },
  { token: "keyword.directive", foreground: "ffa657" },
  { token: "keyword.event", foreground: "d2a8ff" },
  { token: "string", foreground: "a5d6ff" },
  { token: "string.quote", foreground: "a5d6ff" },
  { token: "string.escape", foreground: "79c0ff" },
  { token: "number", foreground: "79c0ff" },
  { token: "number.float", foreground: "79c0ff" },
  { token: "variable", foreground: "79c0ff" },
  { token: "variable.predefined", foreground: "ffa657" },
  { token: "type", foreground: "ffa657" },
  { token: "type.identifier", foreground: "d2a8ff" },
  { token: "operator", foreground: "ff7b72" },
  { token: "delimiter", foreground: "7d8590" },
  { token: "identifier", foreground: "e6edf3" },
];

const LIGHT_TOKEN_RULES: editor.ITokenThemeRule[] = [
  { token: "comment", foreground: "6a737d", fontStyle: "italic" },
  { token: "keyword", foreground: "d73a49" },
  { token: "keyword.directive", foreground: "b08800" },
  { token: "keyword.event", foreground: "6f42c1" },
  { token: "string", foreground: "032f62" },
  { token: "string.quote", foreground: "032f62" },
  { token: "string.escape", foreground: "005cc5" },
  { token: "number", foreground: "005cc5" },
  { token: "number.float", foreground: "005cc5" },
  { token: "variable", foreground: "005cc5" },
  { token: "variable.predefined", foreground: "b08800" },
  { token: "type", foreground: "b08800" },
  { token: "type.identifier", foreground: "6f42c1" },
  { token: "operator", foreground: "d73a49" },
  { token: "delimiter", foreground: "6a737d" },
  { token: "identifier", foreground: "24292e" },
];

const DARK_COLORS: editor.IStandaloneThemeData["colors"] = {
  "editor.background": "#0d1117",
  "editor.foreground": "#e6edf3",
  "editorLineNumber.foreground": "#484f58",
  "editorLineNumber.activeForeground": "#e6edf3",
  "editor.selectionBackground": "#264f78aa",
  "editor.lineHighlightBackground": "#161b22",
  "editorCursor.foreground": "#f97316",
  "editorIndentGuide.background": "#21262d",
  "editorIndentGuide.activeBackground": "#30363d",
  "editorGutter.background": "#0d1117",
  "editorWidget.background": "#161b22",
  "editorWidget.border": "#30363d",
  "editorSuggestWidget.background": "#161b22",
  "editorSuggestWidget.selectedBackground": "#1f6feb33",
  "editorSuggestWidget.highlightForeground": "#f97316",
  "editorHoverWidget.background": "#161b22",
  "editorHoverWidget.border": "#30363d",
  "scrollbarSlider.background": "#30363d55",
  "scrollbarSlider.hoverBackground": "#484f5888",
  "editorBracketMatch.background": "#1f6feb33",
  "editorBracketMatch.border": "#1f6feb",
};

const LIGHT_COLORS: editor.IStandaloneThemeData["colors"] = {
  "editor.background": "#ffffff",
  "editor.foreground": "#24292e",
  "editorLineNumber.foreground": "#959da5",
  "editorLineNumber.activeForeground": "#24292e",
  "editor.selectionBackground": "#0366d625",
  "editor.lineHighlightBackground": "#f6f8fa",
  "editorCursor.foreground": "#f97316",
  "editorIndentGuide.background": "#eaecef",
  "editorIndentGuide.activeBackground": "#d0d7de",
  "editorGutter.background": "#ffffff",
  "editorWidget.background": "#f6f8fa",
  "editorWidget.border": "#d0d7de",
  "editorSuggestWidget.background": "#f6f8fa",
  "editorSuggestWidget.selectedBackground": "#0366d625",
  "editorSuggestWidget.highlightForeground": "#f97316",
  "editorHoverWidget.background": "#f6f8fa",
  "editorHoverWidget.border": "#d0d7de",
  "scrollbarSlider.background": "#d0d7de88",
  "scrollbarSlider.hoverBackground": "#959da5aa",
  "editorBracketMatch.background": "#0366d625",
  "editorBracketMatch.border": "#0366d6",
};

export const DARK_THEME_NAME = "skript-studio-dark";
export const LIGHT_THEME_NAME = "skript-studio-light";

/** Define both themes. Idempotent. */
export function defineSkriptThemes(
  monaco: typeof import("monaco-editor"),
): void {
  monaco.editor.defineTheme(DARK_THEME_NAME, {
    base: "vs-dark",
    inherit: true,
    rules: DARK_TOKEN_RULES,
    colors: DARK_COLORS,
  });

  monaco.editor.defineTheme(LIGHT_THEME_NAME, {
    base: "vs",
    inherit: true,
    rules: LIGHT_TOKEN_RULES,
    colors: LIGHT_COLORS,
  });
}

/** Apply the matching theme by name. */
export function applyTheme(
  monaco: typeof import("monaco-editor"),
  mode: "dark" | "light",
): void {
  monaco.editor.setTheme(
    mode === "dark" ? DARK_THEME_NAME : LIGHT_THEME_NAME,
  );
}
