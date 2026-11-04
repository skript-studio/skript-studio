# Bundled server binaries — NOT committed to git

`bin/skript-lsp-linux-x64` and `bin/skript-lsp-windows-x64.exe` are build
artifacts. They were removed from the repository and its entire git history
(2026-11-04) because committed binaries drift from source and permanently
bloat every clone (~120 MB of blobs after three rebuild generations).

How to produce them locally:

- native slot: `bun run build:binary` (from `extensions/vscode-skript`)
- both slots from Linux: `scripts/cross-build-binaries.sh` (repo root)

CI is being migrated to cross-build the binaries from the merge commit and
attach them as artifacts (see the remediation plan). Until that lands, the
vsix packaging step requires locally built binaries in this directory.

Binaries matching this pattern are gitignored; never commit them again.
