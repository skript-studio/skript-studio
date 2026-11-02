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

/** Name of the bundled binary for the current platform, or "" when unsupported. */
function bundledBinaryName(): string {
  if (process.arch !== "x64") return "";
  switch (process.platform) {
    case "win32":
      return "skript-lsp-windows-x64.exe";
    case "linux":
      return "skript-lsp-linux-x64";
    default:
      return "";
  }
}

function serverPath(context: ExtensionContext): string {
  const configured = workspace
    .getConfiguration("skript-lsp")
    .get<string>("serverPath");
  if (configured) return configured;

  const name = bundledBinaryName();
  if (name) {
    const bundled = path.join(context.extensionPath, "bin", name);
    if (fs.existsSync(bundled)) return bundled;
  }

  // No bundled binary for this platform: fall back to a PATH lookup.
  return "skript-lsp";
}

function buildClient(context: ExtensionContext): LanguageClient {
  const config = workspace.getConfiguration("skript-lsp");
  const logLevel = config.get<string>("logLevel") || "info";

  const binPath = serverPath(context);
  const serverArgs = ["--stdio", "--log-level", logLevel];

  outputChannel.appendLine(`Binary: ${binPath}`);
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

  const configured = workspace
    .getConfiguration("skript-lsp")
    .get<string>("serverPath");
  if (!configured && !bundledBinaryName()) {
    window.showWarningMessage(
      `Skript LSP: no bundled binary for ${process.platform}-${process.arch}; ` +
        'falling back to "skript-lsp" on PATH. ' +
        "Set skript-lsp.serverPath to point at a custom build.",
    );
  }

  client = buildClient(context);

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
        client = buildClient(context);
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
