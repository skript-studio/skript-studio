// Build the skript-lsp release binary for the CURRENT platform and copy it
// into bin/ under the name the extension looks for (see bundledBinaryName()
// in src/extension.ts). Run from anywhere; expects bun or node.
//
//   bun run build:binary
//
// Binary distribution is Windows x64 only: this script only fills the
// win32-x64 slot. On every other platform the extension uses a user-provided
// server (PATH or skript-lsp.serverPath), built with:
//
//   cargo build --release -p skript-lsp
//
// CI builds this same slot from the merge commit on a windows-latest runner
// and downloads it into the vsix packaging job; nothing is ever committed.
import { cpSync, mkdirSync, chmodSync } from "node:fs";
import { execSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import * as path from "node:path";

const extDir = path.dirname(fileURLToPath(new URL(import.meta.url)));
const repoRoot = path.resolve(extDir, "..", "..");

const outName = "skript-lsp-windows-x64.exe";

if (process.platform !== "win32" || process.arch !== "x64") {
  console.error(
    `The extension bundles a server binary for win32-x64 only; ` +
      `${process.platform}-${process.arch} is unsupported.\n` +
      `Build the server for this platform with:\n` +
      `  cargo build --release -p skript-lsp\n` +
      `then point "skript-lsp.serverPath" at target/release/skript-lsp.`,
  );
  process.exit(1);
}

console.log(`Building skript-lsp (release) for win32-x64...`);
execSync("cargo build -p skript-lsp --release", {
  cwd: repoRoot,
  stdio: "inherit",
});

const built = path.join(repoRoot, "target", "release", "skript-lsp.exe");
const dest = path.join(extDir, "bin", outName);
mkdirSync(path.dirname(dest), { recursive: true });
cpSync(built, dest);
chmodSync(dest, 0o755);
console.log(`Copied ${path.relative(repoRoot, built)} -> bin/${outName}`);
