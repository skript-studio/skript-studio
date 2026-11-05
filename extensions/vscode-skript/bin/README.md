# Bundled server binary — NOT committed to git

`bin/skript-lsp-windows-x64.exe` is the only server binary the extension
bundles. It is a build artifact, produced from the merge commit:

- CI: the `binaries` job on `windows-latest` builds it and the
  `vscode-extension` job downloads it before packaging the vsix.
- Locally (Windows x64 only): `bun run build:binary` (from
  `extensions/vscode-skript`).

Linux and macOS users provide the server themselves:

- install `skript-lsp` on `PATH`, or
- set `skript-lsp.serverPath` to a binary built with
  `cargo build --release -p skript-lsp`.

Binaries matching `skript-lsp-*` are gitignored; never commit them again.
