import * as fs from "fs";
import * as path from "path";
import { workspace, ExtensionContext, window, LogOutputChannel } from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  State,
  RevealOutputChannelOn,
} from "vscode-languageclient/node";

let client: LanguageClient | undefined;
let outputChannel: LogOutputChannel;

type ServerSource = "configured" | "bundled" | "path";

interface ServerResolution {
  command: string;
  source: ServerSource;
}

/**
 * Name of the bundled binary for the current platform, or "" when this
 * platform has none. Distribution is Windows x64 only: every other platform
 * uses a user-provided server (PATH or skript-lsp.serverPath).
 */
function bundledBinaryName(
  platform: string = process.platform,
  arch: string = process.arch,
): string {
  return platform === "win32" && arch === "x64"
    ? "skript-lsp-windows-x64.exe"
    : "";
}

/**
 * Warning to show when no bundled server applies, or undefined when one does.
 * Kept free of vscode/process state so the message selection is easy to test.
 */
function fallbackWarning(
  platform: string,
  arch: string,
  bundledExpected: boolean,
  bundledPresent: boolean,
): string | undefined {
  const buildHint =
    'Build it with "cargo build --release -p skript-lsp" and point ' +
    '"skript-lsp.serverPath" at the result.';

  if (bundledExpected) {
    if (bundledPresent) return undefined;
    return (
      `Skript LSP: the bundled server binary is missing from this installation (${platform}-${arch}). ` +
      `Reinstall the extension, set "skript-lsp.serverPath", or ${buildHint}`
    );
  }

  return (
    `Skript LSP: no bundled language server for ${platform}-${arch}. ` +
    "Bundled builds are Windows x64 (win32-x64) only. " +
    `Install "skript-lsp" on your PATH, set "skript-lsp.serverPath", or ${buildHint}`
  );
}

function resolveServer(context: ExtensionContext): ServerResolution {
  const configured = workspace
    .getConfiguration("skript-lsp")
    .get<string>("serverPath")
    ?.trim();
  if (configured) return { command: configured, source: "configured" };

  const name = bundledBinaryName();
  if (name) {
    const bundled = path.join(context.extensionPath, "bin", name);
    if (fs.existsSync(bundled)) return { command: bundled, source: "bundled" };
  }

  // No bundled binary for this platform: fall back to a PATH lookup.
  return { command: "skript-lsp", source: "path" };
}

function buildClient(
  context: ExtensionContext,
  resolution: ServerResolution,
): LanguageClient {
  const config = workspace.getConfiguration("skript-lsp");
  const logLevel = config.get<string>("logLevel") || "info";

  const binPath = resolution.command;
  const serverArgs = ["--stdio", "--log-level", logLevel];

  outputChannel.appendLine(`Binary: ${binPath} (${resolution.source})`);
  outputChannel.appendLine(`Args: ${serverArgs.join(" ")}`);
  outputChannel.appendLine(`Log level: ${logLevel}`);

  const serverOptions: ServerOptions = {
    command: binPath,
    args: serverArgs,
    options: {
      env: { ...process.env, RUST_LOG: logLevel },
    },
  };

  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ language: "skript" }],
    synchronize: {
      configurationSection: "skript-lsp",
    },
    initializationOptions: {
      maxCompletions: config.get<number>("maxCompletions", 100),
      parserDiagnostics: config.get<string>("parserDiagnostics", "hint"),
    },
    outputChannel,
    revealOutputChannelOn: RevealOutputChannelOn.Never,
  };

  return new LanguageClient("skript-lsp", "Skript LSP", serverOptions, clientOptions);
}

export function activate(context: ExtensionContext) {
  outputChannel = window.createOutputChannel("Skript LSP", { log: true });

  outputChannel.appendLine("Activating Skript LSP extension...");

  const resolution = resolveServer(context);
  if (resolution.source === "path") {
    const expectedName = bundledBinaryName();
    const present =
      expectedName !== "" &&
      fs.existsSync(path.join(context.extensionPath, "bin", expectedName));
    const warning = fallbackWarning(
      process.platform,
      process.arch,
      expectedName !== "",
      present,
    );
    if (warning) window.showWarningMessage(warning);
  }

  client = buildClient(context, resolution);

  client.onDidChangeState((e) => {
    const states: Record<number, string> = {
      [State.Starting]: "Starting",
      [State.Running]: "Running",
      [State.Stopped]: "Stopped",
    };
    outputChannel.appendLine(`State change: ${states[e.newState] ?? e.newState}`);
  });

  context.subscriptions.push(
    workspace.onDidChangeConfiguration(async (e) => {
      if (e.affectsConfiguration("skript-lsp")) {
        outputChannel.appendLine("Configuration changed; restarting LSP...");
        await stopClient();
        client = buildClient(context, resolveServer(context));
        client.start();
      }
    }),
  );

  client.start();
}

async function stopClient() {
  if (client) {
    try {
      await client.stop();
    } catch {
      // ignore stop errors
    }
  }
}

export function deactivate(): Thenable<void> | undefined {
  if (!client) {
    return undefined;
  }
  return client.stop();
}
