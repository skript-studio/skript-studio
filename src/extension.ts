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

function serverPath(context: ExtensionContext): string {
  const configured = workspace
    .getConfiguration("skript-lsp")
    .get<string>("serverPath");
  if (configured) return configured;

  const extDir = context.extensionPath;
  const binary = process.platform === "win32" ? "skript-lsp.exe" : "skript-lsp";
  const bundled = path.join(extDir, "bin", binary);
  if (require("fs").existsSync(bundled)) return bundled;

  return binary;
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
    },
    outputChannel,
    revealOutputChannelOn: RevealOutputChannelOn.Never,
  };

  return new LanguageClient("skript-lsp", "Skript LSP", serverOptions, clientOptions);
}

export function activate(context: ExtensionContext) {
  outputChannel = window.createOutputChannel("Skript LSP", { log: true });

  outputChannel.appendLine("Activating Skript LSP extension...");

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
