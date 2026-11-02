#!/usr/bin/env bash
# Cross-build both bundled skript-lsp binaries from an x86_64 Linux host
# (developed and verified on the Z.ai sandbox: Debian 13, glibc 2.41):
#
#   bin/skript-lsp-linux-x64        static musl build, runs on any x86_64 Linux
#   bin/skript-lsp-windows-x64.exe  MinGW-w64 build, runs on x86_64 Windows
#
# The Windows binary can equally be produced natively on a Windows PC with
# `bun run build:binary` in extensions/vscode-skript (MSVC toolchain) and
# committed over the cross-built one — the slot name is identical.
#
# Requirements: rustup toolchain on PATH (source env.sh), network access to
# deb.debian.org, no root. Downloads four Debian packages into
# .toolchain/mingw (binutils, mingw-w64 runtime, libgcc) and links with the
# rust-lld bundled in the Rust toolchain.
set -euo pipefail

REPO=$(cd "$(dirname "$0")/.." && pwd)
# shellcheck source=../../env.sh
source "$REPO/env.sh"

TOOLCHAIN_DIR=${TOOLCHAIN_DIR:-/home/z/my-project/.toolchain}
MINGW=$TOOLCHAIN_DIR/mingw
SYSROOT=$(rustc --print sysroot)
LLD=$SYSROOT/lib/rustlib/x86_64-unknown-linux-gnu/bin/rust-lld

rustup target add x86_64-unknown-linux-musl x86_64-pc-windows-gnu

# --- MinGW link support (dlltool, import libs, libgcc) ----------------------
mkdir -p "$MINGW"
cd "$MINGW"
for pkg in binutils-mingw-w64-x86-64 mingw-w64-x86-64-dev mingw-w64-common gcc-mingw-w64-x86-64-posix; do
  if ! ls "$pkg"_*.deb >/dev/null 2>&1; then
    apt-get download "$pkg"
    dpkg -x "$pkg"_*.deb "$MINGW"
  fi
done
GCC_LIB=$(find "$MINGW/usr/lib/gcc/x86_64-w64-mingw32" -mindepth 1 -maxdepth 1 -type d | sort | tail -1)
export PATH="$MINGW/usr/bin:$PATH"

cd "$REPO"
mkdir -p extensions/vscode-skript/bin

# --- Linux: static musl, no glibc version coupling --------------------------
cargo build -p skript-lsp --release --target x86_64-unknown-linux-musl
cp target/x86_64-unknown-linux-musl/release/skript-lsp \
  extensions/vscode-skript/bin/skript-lsp-linux-x64
chmod +x extensions/vscode-skript/bin/skript-lsp-linux-x64

# --- Windows: MinGW via rust-lld + Debian mingw runtime ---------------------
CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER="$LLD" \
CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUSTFLAGS="-C link-self-contained=yes \
-L native=$MINGW/usr/x86_64-w64-mingw32/lib \
-L native=$GCC_LIB" \
cargo build -p skript-lsp --release --target x86_64-pc-windows-gnu
cp target/x86_64-pc-windows-gnu/release/skript-lsp.exe \
  extensions/vscode-skript/bin/skript-lsp-windows-x64.exe

ls -lh extensions/vscode-skript/bin/
