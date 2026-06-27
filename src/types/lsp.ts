/**
 * LSP-related types. Most protocol objects come from `vscode-languageserver-protocol`
 * (transitively via `monaco-languageclient`); these are the custom types
 * we use on the frontend to track LSP lifecycle and the data we exchange
 * with the Tauri backend commands.
 */

/** LSP process status, returned by `get_lsp_status`. */
export type LSPStatus =
  | "connected"
  | "starting"
  | "disconnected"
  | "error";

/** Result of `start_lsp` and `restart_lsp` commands. */
export interface LSPStartResult {
  port: number;
}

/** Result of `get_lsp_status`. */
export interface LSPStatusResult {
  status: LSPStatus;
  /** Present when status is `connected` or `starting`. */
  port?: number;
  /** Human-readable error message when status is `error`. */
  error?: string;
}

/** A single log line emitted by the LSP sidecar. */
export interface LSPLogEntry {
  timestamp: number;
  level: "info" | "warn" | "error" | "debug";
  message: string;
}

/** Diagnostic severity (mirrors LSP `DiagnosticSeverity`). */
export type DiagnosticSeverity = 1 | 2 | 3 | 4;

export interface Diagnostic {
  startLine: number;
  endLine: number;
  startColumn: number;
  endColumn: number;
  severity: DiagnosticSeverity;
  message: string;
  source?: string;
}

/**
 * Known diagnostic codes emitted by the skript-lsp server. The server
 * always sets `source: "skript"` and uses these literal code strings.
 * Used by the OutputPanel / future Problems panel to render friendly
 * messages and pick the right icon.
 */
export type SkriptDiagnosticCode =
  | "unknown-syntax"
  | "removed-syntax"
  | "requires-plugin"
  | "type-mismatch-N";

export const SKRIPT_DIAGNOSTIC_SOURCE = "skript";

export const DIAGNOSTIC_CODE_LABELS: Record<string, string> = {
  "unknown-syntax": "Unknown syntax",
  "removed-syntax": "Removed syntax",
  "requires-plugin": "Missing plugin",
  "type-mismatch-N": "Type mismatch",
};

/** Convert a diagnostic code into a short, human-readable label. */
export function describeDiagnosticCode(code: string | number | undefined): string | null {
  if (code == null) return null;
  const key = String(code);
  if (key in DIAGNOSTIC_CODE_LABELS) return DIAGNOSTIC_CODE_LABELS[key];
  // type-mismatch-N is parameterised — strip the trailing number.
  if (key.startsWith("type-mismatch-")) return "Type mismatch";
  return key;
}

/**
 * The 9 semantic token types the LSP server advertises, in the exact
 * order it returns them in `SemanticTokensLegend.tokenTypes`. Used to
 * verify our Monarch fallback tokens line up with what the LSP sends.
 *
 * Per backend contract #5: no modifiers in v1.
 */
export const SKRIPT_SEMANTIC_TOKEN_TYPES = [
  "keyword",
  "type",
  "variable",
  "string",
  "comment",
  "operator",
  "event",
  "function",
  "number",
] as const;

export type SkriptSemanticTokenType = (typeof SKRIPT_SEMANTIC_TOKEN_TYPES)[number];
