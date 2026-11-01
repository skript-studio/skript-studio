/**
 * TypeScript types for SkriptHub data and the Skript language domain.
 * Mirrors the `SyntaxEntry` struct in the Rust `skript-core` library.
 */

export type SkriptSyntaxType =
  | "event"
  | "effect"
  | "condition"
  | "expression"
  | "section"
  | "other";

export const SYNTAX_TYPE_LABELS: Record<SkriptSyntaxType, string> = {
  event: "Event",
  effect: "Effect",
  condition: "Condition",
  expression: "Expression",
  section: "Section",
  other: "Other",
};

export const SYNTAX_TYPE_COLORS: Record<SkriptSyntaxType, string> = {
  event: "bg-purple-500/15 text-purple-500 dark:text-purple-300 border-purple-500/30",
  effect: "bg-blue-500/15 text-blue-500 dark:text-blue-300 border-blue-500/30",
  condition:
    "bg-amber-500/15 text-amber-600 dark:text-amber-300 border-amber-500/30",
  expression:
    "bg-cyan-500/15 text-cyan-600 dark:text-cyan-300 border-cyan-500/30",
  section:
    "bg-orange-500/15 text-orange-600 dark:text-orange-300 border-orange-500/30",
  other: "bg-zinc-500/15 text-zinc-500 dark:text-zinc-300 border-zinc-500/30",
};

/**
 * A single entry in the SkriptHub addon syntax database.
 * Returned by the LSP (cached from https://skripthub.net/api/v1/addonsyntaxlist/)
 * or fetched directly by the frontend.
 */
export interface SkriptHubEntry {
  /** SkriptHub numeric ID. */
  id: number;
  /** Human-readable title, e.g. "On Join". */
  title: string;
  /** Long-form documentation. */
  description: string;
  /** Canonical syntax pattern. May contain `\r\n` for multiple variants. */
  syntax_pattern: string;
  syntax_type: SkriptSyntaxType;
  /** Addon that ships this syntax (Skript itself counts as an addon). */
  addon: {
    name: string;
    link_to_addon: string;
    /** 0..100 — relative popularity from community usage telemetry. */
    usage_score: number;
  };
  /** Return type for expressions, `null` for other syntax kinds. */
  return_type: string | null;
  /** Required plugins for this syntax to function. */
  required_plugins: Array<{
    name: string;
    link: string;
  }>;
  /** Comma-separated list of event values (only for `event` syntax type). */
  event_values: string | null;
  /** Whether the event can be cancelled (only for `event` syntax type). */
  event_cancellable: boolean;
  /** Whether this syntax has been removed from the addon. */
  mark_as_removed: boolean;
  /** Addon version in which this syntax was removed, if any. */
  removed_since: string | null;
  /** Minimum addon version required to use this syntax. */
  compatible_addon_version: string;
}

/** Variant of a syntax pattern, split from `syntax_pattern` on `\r\n`. */
export interface PatternVariant {
  raw: string;
  /** Tokenized pattern with `<required>`, `[optional]`, `(a|b)` highlighted. */
  segments: PatternSegment[];
}

export type PatternSegment =
  | { kind: "text"; value: string }
  | { kind: "required"; value: string }
  | { kind: "optional"; value: string }
  | { kind: "choice"; options: string[] };

/** Split a SkriptHub pattern string into highlightable segments. */
export function parsePattern(pattern: string): PatternVariant[] {
  return pattern.split(/\r?\n/).map((line) => ({
    raw: line,
    segments: tokenizePattern(line),
  }));
}

function tokenizePattern(line: string): PatternSegment[] {
  const segments: PatternSegment[] = [];
  let i = 0;
  let buf = "";
  const flush = () => {
    if (buf) {
      segments.push({ kind: "text", value: buf });
      buf = "";
    }
  };

  while (i < line.length) {
    const c = line[i];
    if (c === "<") {
      flush();
      const end = line.indexOf(">", i);
      if (end === -1) {
        buf += c;
        i++;
        continue;
      }
      segments.push({ kind: "required", value: line.slice(i + 1, end) });
      i = end + 1;
    } else if (c === "[") {
      flush();
      const end = line.indexOf("]", i);
      if (end === -1) {
        buf += c;
        i++;
        continue;
      }
      segments.push({ kind: "optional", value: line.slice(i + 1, end) });
      i = end + 1;
    } else if (c === "(") {
      flush();
      const end = line.indexOf(")", i);
      if (end === -1) {
        buf += c;
        i++;
        continue;
      }
      const inner = line.slice(i + 1, end);
      segments.push({ kind: "choice", options: inner.split("|") });
      i = end + 1;
    } else {
      buf += c;
      i++;
    }
  }
  flush();
  return segments;
}
