#!/usr/bin/env bash
# Cross-compiles the portable Windows build on Linux (MinGW-w64).
#
# Requirements (Ubuntu 24.04): mingw-w64, Rust >= 1.80 and the Rust standard
# library sources (rustup component "rust-src", or the distro package
# rust-1.xx-src linked into the toolchain sysroot) for -Zbuild-std. With
# rustup and internet access you can simply use instead:
#     rustup target add x86_64-pc-windows-gnu
#     cargo build --release -p game-curator --target x86_64-pc-windows-gnu
# The recommended way is still a native build on Windows (see docs/BUILD.md).
set -euo pipefail
cd "$(dirname "$0")/.."
TARGET=x86_64-pc-windows-gnu
if rustup target list --installed 2>/dev/null | grep -q "$TARGET"; then
  cargo build --release -p game-curator --target "$TARGET"
else
  RUSTC_BOOTSTRAP=1 cargo build --release -p game-curator --target "$TARGET" -Zbuild-std=std,panic_abort
fi
echo "Built: target/$TARGET/release/GameCurator.exe"
