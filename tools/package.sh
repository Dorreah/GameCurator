#!/usr/bin/env bash
# Assembles the portable distribution: dist/GAME_CURATOR/ + dist/GameCurator-0.1.0-windows-portable.zip
set -euo pipefail
cd "$(dirname "$0")/.."
EXE="${1:-target/x86_64-pc-windows-gnu/release/GameCurator.exe}"
[ -f "$EXE" ] || EXE=target/release/GameCurator.exe
[ -f "$EXE" ] || { echo "GameCurator.exe not found - build first"; exit 1; }
rm -rf dist && mkdir -p dist/GAME_CURATOR
cp "$EXE" dist/GAME_CURATOR/GameCurator.exe
cp portable/README.txt dist/GAME_CURATOR/README.txt
# MinGW (GNU) builds load the WebView2 loader dynamically: ship Microsoft's
# redistributable WebView2Loader.dll next to the exe. MSVC builds link it
# statically and do not need it.
IMPORTS=$(x86_64-w64-mingw32-objdump -p dist/GAME_CURATOR/GameCurator.exe 2>/dev/null || true)
if grep -q "WebView2Loader.dll" <<<"$IMPORTS"; then
  DLL=$(find "${CARGO_HOME:-$HOME/.cargo}/registry/src" -path "*webview2-com-sys-*/x64/WebView2Loader.dll" | sort | tail -1)
  cp "$DLL" dist/GAME_CURATOR/WebView2Loader.dll
fi
# Data/, Games/ and .gamecurator-id are created by the app on first start.
(cd dist && rm -f GameCurator-0.1.0-windows-portable.zip && zip -qr GameCurator-0.1.0-windows-portable.zip GAME_CURATOR)
ls -la dist dist/GAME_CURATOR
