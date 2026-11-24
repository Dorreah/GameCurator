#!/usr/bin/env bash
# Assembles the portable distribution: dist/GAME_CURATOR/ + dist/GameCurator-0.2.0-windows-portable.zip
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
# Licence texts of the fonts and of pdf.js (the PDF viewer), which are built into the exe.
mkdir -p dist/GAME_CURATOR/Licenses
cp ui/fonts/LICENSE-*.txt dist/GAME_CURATOR/Licenses/
P=ui/vendor/pdfjs
cp "$P/LICENSE" dist/GAME_CURATOR/Licenses/LICENSE-pdfjs.txt
cp "$P/standard_fonts/LICENSE_FOXIT" dist/GAME_CURATOR/Licenses/LICENSE-pdfjs-Foxit-fonts.txt
cp "$P/standard_fonts/LICENSE_LIBERATION" dist/GAME_CURATOR/Licenses/LICENSE-pdfjs-Liberation-fonts.txt
cp "$P/iccs/LICENSE" dist/GAME_CURATOR/Licenses/LICENSE-pdfjs-ICC-profile.txt
for f in JBIG2 OPENJPEG QCMS PDFJS_JBIG2 PDFJS_OPENJPEG PDFJS_QCMS; do
  cp "$P/wasm/LICENSE_$f" "dist/GAME_CURATOR/Licenses/LICENSE-pdfjs-wasm-$f.txt"
done
# Data/, Games/ and .gamecurator-id are created by the app on first start.
(cd dist && rm -f GameCurator-0.2.0-windows-portable.zip && zip -qr GameCurator-0.2.0-windows-portable.zip GAME_CURATOR)
ls -la dist dist/GAME_CURATOR
