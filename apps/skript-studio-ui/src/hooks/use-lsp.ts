/**
 * use-lsp — initialise the Monaco language client over a WebSocket
 * connection to the skript-lsp sidecar managed by Tauri.
 *
 * Flow:
 *  1. Read settings (SkriptHub URL, log level, max completions) from
 *     the settings store.
 *  2. Tauri backend spawns the skript-lsp sidecar with matching CLI
 *     flags (--cache-dir, --log-level, --skripthub-url) and returns
 *     the port it's listening on.
 *  3. We open a WebSocket to `ws://127.0.0.1:<port>`.
 *  4. On open, we wrap the socket in `WebSocketMessageReader` / `Writer`
 *     and start a `MonacoLanguageClient` with:
 *       - documentSelector for "skript"
 *       - initializationOptions { skriptHubUrl, logLevel, maxCompletions }
 *       - disabled capabilities for methods the server doesn't implement
 *         (definition, references, rename, formatting, codeAction,
 *         signatureHelp) so we don't get `method not found` noise.
 *  5. On settings changes, push `workspace/didChangeConfiguration` with
 *     section `"skript"` so the server picks up new log level / URL.
 *  6. On unmount / restart, send the LSP `shutdown` → `exit` handshake
 *     before the Rust side hard-kills the process.
 *
 * In browser dev mode (no Tauri), the hook no-ops and logs a warning —
 * the editor still works with the local Monarch tokenizer only.
 */

import { useEffect, useRef, useState } from "react";
import { MonacoLanguageClient } from "monaco-languageclient";
import {
  toSocket,
  WebSocketMessageReader,
  WebSocketMessageWriter,
} from "vscode-ws-jsonrpc";
import type * as Monaco from "monaco-editor";
import { useLSPStore } from "@/stores/lsp-store";
import { useSettingsStore } from "@/stores/settings-store";
import { useTauriLSP } from "./use-tauri-lsp";
import { SKRIPT_LANGUAGE_ID } from "@/lib/monaco-skript";
import type { MessageTransports } from "vscode-languageclient";
import type { LspSpawnOptions } from "@/lib/tauri-commands";

interface UseLSPArgs {
  /** Monaco namespace — passed in once the editor mounts. */
  monaco: typeof Monaco | null;
}

interface UseLSPResult {
  /** True when the language client is started and ready. */
  ready: boolean;
  /** Last error, if any. */
  error: string | null;
  /** Manually restart the LSP connection. */
  restart: () => Promise<void>;
}

/**
 * Build the LSP `initializationOptions` payload from the settings store.
 * Mirrors the server's schema: { skriptHubUrl?, logLevel?, maxCompletions? }.
 */
function buildInitOptions(): Record<string, unknown> {
  const settings = useSettingsStore.getState().lsp;
  const opts: Record<string, unknown> = {};
  if (settings.skripthubApiUrl) opts.skriptHubUrl = settings.skripthubApiUrl;
  if (settings.logLevel) opts.logLevel = settings.logLevel;
  // maxCompletions isn't currently in the settings UI; omit if unset.
  return opts;
}

/** Build the sidecar spawn options (forwarded as CLI flags). */
function buildSpawnOptions(): LspSpawnOptions {
  const settings = useSettingsStore.getState().lsp;
  const opts: LspSpawnOptions = {
    logLevel: settings.logLevel,
  };
  if (settings.skripthubApiUrl) opts.skripthubUrl = settings.skripthubApiUrl;
  return opts;
}

/** Push live settings changes to the server via workspace/didChangeConfiguration. */
function pushConfigChange(client: MonacoLanguageClient): void {
  const settings = useSettingsStore.getState().lsp;
  // The LSP server pulls { skriptHubUrl, logLevel, maxCompletions } via
  // workspace/configuration with section "skript". We mirror that shape.
  client.sendNotification("workspace/didChangeConfiguration", {
    settings: {
      skript: {
        skriptHubUrl: settings.skripthubApiUrl,
        logLevel: settings.logLevel,
        // maxCompletions omitted — same as initializationOptions
      },
    },
  });
}

export function useLSP({ monaco }: UseLSPArgs): UseLSPResult {
  const [ready, setReady] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const clientRef = useRef<MonacoLanguageClient | null>(null);
  const socketRef = useRef<WebSocket | null>(null);

  const tauriLSP = useTauriLSP();
  const setStatus = useLSPStore((s) => s.setStatus);
  const setPort = useLSPStore((s) => s.setPort);

  /** Establish the WebSocket + start the language client. */
  const connect = async (port: number) => {
    if (!monaco) return;
    const url = `ws://127.0.0.1:${port}`;
    const socket = new WebSocket(url);
    socketRef.current = socket;

    socket.onopen = () => {
      const rpcSocket = toSocket(socket);
      const reader = new WebSocketMessageReader(rpcSocket);
      const writer = new WebSocketMessageWriter(rpcSocket);

      const transports: MessageTransports = { reader, writer };

      const client = new MonacoLanguageClient({
        name: "Skript LSP",
        clientOptions: {
          documentSelector: [{ language: SKRIPT_LANGUAGE_ID }],
          // Mirror initializationOptions so the server gets the same
          // SkriptHub URL / log level it received via CLI flags.
          initializationOptions: buildInitOptions(),
          // The server only advertises: completion, hover, diagnostics
          // (push), semanticTokens/full, textDocumentSync (FULL).
          // vscode-languageclient automatically skips requests for
          // capabilities the server didn't advertise in its
          // `initialize` response — no need to disable them here.
          // LSP 3.17 — server targets 3.17 with absolute (delta-less)
          // semantic token positions.
          errorHandler: {
            // On transport close, mark as disconnected. The frontend's
            // 5s status poll will then surface the state to the user,
            // who can click "Restart" if autoRestart is off.
            error: () => ({
              action: 1 as const, // Continue
            }),
            closed: () => ({
              action: 1 as const, // DoNotRestart — Tauri owns restart
            }),
          },
        },
        connectionProvider: {
          get: async () => transports,
        },
      });

      clientRef.current = client;
      client
        .start()
        .then(() => {
          setReady(true);
          setStatus("connected");
        })
        .catch((err: unknown) => {
          setError(err instanceof Error ? err.message : String(err));
          setStatus("error", err instanceof Error ? err.message : String(err));
        });

      reader.onClose(() => {
        setReady(false);
        setStatus("disconnected");
      });
    };

    socket.onerror = (ev) => {
      const msg = `WebSocket error connecting to LSP at ${url}`;
      console.error(msg, ev);
      setError(msg);
      setStatus("error", msg);
      setReady(false);
    };
  };

  /** Send the LSP `shutdown` → `exit` handshake before killing. */
  const gracefulShutdown = async (): Promise<void> => {
    const client = clientRef.current;
    if (!client) return;
    try {
      // LSP 3.17: shutdown is a request, exit is a notification.
      // The server implements the full handshake gracefully per
      // backend contract #10. Wrap in a 2s timeout — if the server
      // doesn't respond, the Rust hard-kill is the safety net.
      await Promise.race([
        client.sendRequest("shutdown"),
        new Promise((_, reject) =>
          setTimeout(() => reject(new Error("shutdown timeout")), 2000),
        ),
      ]);
      client.sendNotification("exit");
    } catch (err) {
      console.warn("[use-lsp] graceful shutdown failed, will hard-kill:", err);
    }
  };

  // Start the LSP once Monaco is ready
  useEffect(() => {
    if (!monaco) return;
    if (!tauriLSP.available) {
      console.warn(
        "[use-lsp] Tauri backend unavailable — running without LSP. " +
          "Editor will use the local Monarch tokenizer only.",
      );
      setStatus("disconnected");
      return;
    }

    let cancelled = false;
    (async () => {
      setStatus("starting");
      try {
        await tauriLSP.start(buildSpawnOptions());
        const port = useLSPStore.getState().port;
        if (port != null && !cancelled) {
          await connect(port);
        }
      } catch (err) {
        if (!cancelled) {
          setError(err instanceof Error ? err.message : String(err));
          setStatus("error", err instanceof Error ? err.message : String(err));
        }
      }
    })();

    return () => {
      cancelled = true;
      // Initiate graceful shutdown, then close the socket. The Rust
      // side will hard-kill as a safety net when stop_lsp is called.
      void gracefulShutdown().then(() => {
        if (clientRef.current) {
          void clientRef.current.dispose();
          clientRef.current = null;
        }
        if (socketRef.current) {
          socketRef.current.close();
          socketRef.current = null;
        }
      });
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [monaco, tauriLSP.available]);

  // Watch settings store and push changes to the running LSP via
  // workspace/didChangeConfiguration. The server pulls
  // { skriptHubUrl, logLevel, maxCompletions } under section "skript".
  useEffect(() => {
    const unsub = useSettingsStore.subscribe((state, prev) => {
      const client = clientRef.current;
      if (!client || !ready) return;
      if (state.lsp === prev.lsp) return;
      try {
        pushConfigChange(client);
      } catch (err) {
        console.warn("[use-lsp] failed to push config change:", err);
      }
    });
    return unsub;
  }, [ready]);

  const restart = async () => {
    // 1. Tell the server to shut down gracefully
    await gracefulShutdown();
    if (clientRef.current) {
      void clientRef.current.dispose();
      clientRef.current = null;
    }
    if (socketRef.current) {
      socketRef.current.close();
      socketRef.current = null;
    }
    setReady(false);
    setStatus("starting");
    // 2. Restart the sidecar (backend re-applies the cached spawn options)
    await tauriLSP.restart();
    const port = useLSPStore.getState().port;
    if (port != null) {
      await connect(port);
    }
  };

  return { ready, error, restart };
}
