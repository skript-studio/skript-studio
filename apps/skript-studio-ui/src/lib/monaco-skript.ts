/**
 * Skript language registration for Monaco.
 *
 * Registers the `skript` language ID with a Monarch tokenizer covering
 * keywords, types, operators, variables (`{x}`, `{_x}`), color codes
 * (`&a`), comments (`#`), and string literals. The token types produced
 * here are the same ones the LSP returns as semantic tokens, so the
 * editor's syntax highlighting stays consistent between the local
 * tokenizer (instant) and the LSP (which may lag).
 */

import type { languages } from "monaco-editor";

export const SKRIPT_LANGUAGE_ID = "skript";

const KEYWORDS = [
  "on", "if", "else", "loop", "while", "for", "set", "delete", "add", "remove",
  "function", "command", "trigger", "options", "return", "stop", "exit",
  "continue", "cancel", "broadcast", "send", "make", "spawn", "teleport",
  "heal", "damage", "give", "execute", "wait", "play", "run", "with", "at",
  "to", "from", "into", "of", "in", "named", "where", "by", "as",
];

const TYPES = [
  "player", "number", "text", "string", "location", "entity", "item",
  "boolean", "world", "gamemode", "material", "block", "object", "inventory",
];

const OPERATORS = [
  ">=", "<=", "!=", "==", ">", "<", "=", "and", "or", "not", "is", "isn't",
  "contains", "between",
];

const EVENTS = [
  "join", "quit", "connect", "disconnect", "death", "respawn", "chat",
  "break", "place", "right click", "left click", "script load", "server load",
  "shoot", "drop", "pickup", "click", "sneak", "sprint", "walk", "step on",
  "fall", "fly", "glide",
];

/** Register the skript language with Monaco. Idempotent. */
export function registerSkriptLanguage(
  monaco: typeof import("monaco-editor"),
): void {
  if (
    monaco.languages.getLanguages().some((l) => l.id === SKRIPT_LANGUAGE_ID)
  ) {
    return;
  }

  monaco.languages.register({
    id: SKRIPT_LANGUAGE_ID,
    extensions: [".sk"],
    aliases: ["Skript", "skript"],
  });

  monaco.languages.setMonarchTokensProvider(SKRIPT_LANGUAGE_ID, {
    defaultToken: "",
    tokenPostfix: ".skript",
    ignoreCase: true,
    brackets: [
      { open: "{", close: "}", token: "delimiter.curly" },
      { open: "[", close: "]", token: "delimiter.square" },
      { open: "(", close: ")", token: "delimiter.parenthesis" },
    ],
    keywords: KEYWORDS,
    typeKeywords: TYPES,
    operators: OPERATORS,
    events: EVENTS,
    symbols: /[=><!~?:&|+\-*/^%]+/,
    tokenizer: {
      root: [
        // Comments
        [/#.*$/, "comment"],

        // Section headers
        [
          /(command|options|variables|aliases|function)\b(?=.*:)/,
          "keyword.directive",
        ],
        [/\/[a-zA-Z][\w-]*/, "type.identifier"],

        // Color codes
        [/&[0-9a-fk-orA-FK-OR]/, "string.escape"],

        // Variables — global {x}, local {_x}
        [/\{[a-zA-Z_][\w:]*\}/, "variable"],
        [/\{_[a-zA-Z_][\w:]*\}/, "variable.predefined"],

        // Strings (single and double; Skript also supports paired single-quotes)
        [/"/, { token: "string.quote", next: "@string_double" }],
        [/'/, { token: "string.quote", next: "@string_single" }],

        // Numbers
        [/-?\d+\.\d+/, "number.float"],
        [/-?\d+/, "number"],

        // Section terminator
        [/:$/, "delimiter"],

        // Identifiers & whitespace
        [
          /[a-zA-Z_][\w\s]*/,
          {
            cases: {
              "@keywords": "keyword",
              "@typeKeywords": "type",
              "@events": "keyword.event",
              "@operators": "operator",
              "@default": "identifier",
            },
          },
        ],
        [/[{}()\[\]]/, "@brackets"],
        [
          /@symbols/,
          { cases: { "@operators": "operator", "@default": "" } },
        ],
        [/[ \t\r\n]+/, ""],
      ],
      string_double: [
        [/[^"\\]+/, "string"],
        [/\\./, "string.escape"],
        [/&[0-9a-fk-orA-FK-OR]/, "string.escape"],
        [/"/, { token: "string.quote", next: "@pop" }],
      ],
      string_single: [
        [/[^'\\]+/, "string"],
        [/\\./, "string.escape"],
        [/&[0-9a-fk-orA-FK-OR]/, "string.escape"],
        [/'/, { token: "string.quote", next: "@pop" }],
      ],
    },
  } as languages.IMonarchLanguage);

  monaco.languages.setLanguageConfiguration(SKRIPT_LANGUAGE_ID, {
    comments: { lineComment: "#" },
    brackets: [
      ["{", "}"],
      ["[", "]"],
      ["(", ")"],
    ],
    autoClosingPairs: [
      { open: "{", close: "}" },
      { open: "[", close: "]" },
      { open: "(", close: ")" },
      { open: '"', close: '"' },
      { open: "'", close: "'" },
    ],
    surroundingPairs: [
      { open: "{", close: "}" },
      { open: "[", close: "]" },
      { open: "(", close: ")" },
      { open: '"', close: '"' },
      { open: "'", close: "'" },
    ],
    indentationRules: {
      increaseIndentPattern: /:\s*$/,
      decreaseIndentPattern: /^\s*(else|else if)\b/,
    },
  });
}
