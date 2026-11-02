// Build the skript-lsp release binary for the CURRENT platform and copy it
// into bin/ under the name the extension looks for (see bundledBinaryName()
// in src/extension.ts). Run from anywhere; expects bun or node.
//
//   bun run build:binary
//
// Cross-compiling both bundled binaries from an x86_64 Linux host is possible
// with scripts/cross-build-binaries.sh at the repo root. On Windows this
// script produces an MSVC build that simply overwrites the committed
// cross-built exe in bin/.
import { cpSync, mkdirSync, chmodSync } from "node:fs";
import { execSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import * as path from "node:path";

const extDir = path.dirname(fileURLToPath(new URL(import.meta.url)));
const repoRoot = path.resolve(extDir, "..", "..");

const slots = {
  "win32-x64": "skript-lsp-windows-x64.exe",
  "linux-x64": "skript-lsp-linux-x64",
};

const key = `${process.platform}-${process.arch}`;
const outName = slots[key];
if (!outName) {
  console.error(
    `No bundled binary slot for ${key}; this extension bundles win32-x64 and linux-x64 only.`,
  );
  process.exit(1);
}

console.log(`Building skript-lsp (release) for ${key}...`);
execSync("cargo build -p skript-lsp --release", {
  cwd: repoRoot,
  stdio: "inherit",
});

const built = path.join(
  repoRoot,
  "target",
  "release",
  process.platform === "win32" ? "skript-lsp.exe" : "skript-lsp",
);
const dest = path.join(extDir, "bin", outName);
mkdirSync(path.dirname(dest), { recursive: true });
cpSync(built, dest);
if (process.platform !== "win32") chmodSync(dest, 0o755);
console.log(`Copied ${path.relative(repoRoot, built)} -> bin/${outName}`);
