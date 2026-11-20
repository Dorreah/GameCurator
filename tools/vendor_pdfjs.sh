#!/usr/bin/env bash
# Copies the parts of pdf.js that the built-in PDF viewer needs into
# ui/vendor/pdfjs. Game Curator is offline, so pdf.js ships inside the exe.
#
#   npm pack pdfjs-dist@6.5.438 && tar -xzf pdfjs-dist-6.5.438.tgz   (-> package/)
#   ./tools/vendor_pdfjs.sh package
#
# Left out on purpose: the scripting sandbox (QuickJS - PDF JavaScript is
# never run), CJK character maps, the renderer worker and the viewer app.
set -euo pipefail
src="${1:?path to the extracted pdfjs-dist package}"
here="$(cd "$(dirname "$0")/.." && pwd)"
dst="$here/ui/vendor/pdfjs"
rm -rf "$dst"
mkdir -p "$dst/standard_fonts" "$dst/wasm" "$dst/iccs"
# the "legacy" build: the same pdf.js with polyfills for features a
# WebView2 runtime that has not updated yet may lack
cp "$src/legacy/build/pdf.min.mjs" "$src/legacy/build/pdf.worker.min.mjs" "$dst/"
cp "$src/LICENSE" "$dst/LICENSE"
cp "$src"/standard_fonts/* "$dst/standard_fonts/"
cp "$src"/iccs/* "$dst/iccs/"
for f in jbig2.wasm jbig2_nowasm_fallback.js openjpeg.wasm openjpeg_nowasm_fallback.js qcms_bg.wasm \
         LICENSE_JBIG2 LICENSE_OPENJPEG LICENSE_PDFJS_JBIG2 LICENSE_PDFJS_OPENJPEG LICENSE_PDFJS_QCMS LICENSE_QCMS; do
  cp "$src/wasm/$f" "$dst/wasm/"
done
version="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["version"])' "$src/package.json")"
echo "pdf.js (pdfjs-dist) $version - https://github.com/mozilla/pdf.js - Apache License 2.0" > "$dst/VERSION.txt"
du -sh "$dst"
