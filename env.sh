# source this before any cargo command in this workspace
# NOTE: .toolchain lives in /tmp (too large/slow to relocate to /home/z).
# A tar snapshot is kept at /home/z/my-project/.toolchain.tar; if /tmp is
# wiped, extract it back with:
#   mkdir -p /tmp/my-project && tar -xf /home/z/my-project/.toolchain.tar -C /tmp/my-project
export RUSTUP_HOME=/tmp/my-project/.toolchain/rustup
export CARGO_HOME=/tmp/my-project/.toolchain/cargo
export PATH="/tmp/my-project/.toolchain/cargo/bin:$PATH"
