"""Builds the pixel web fonts of the Classic and DOS Midnight themes.

Classic: Pixel Operator (CC0, by Jayvee Enaguas), tools/fonts-src/pixel-operator.
  Pixel Operator has no glyphs for a few symbols the interface uses (▶ ◄ ✕ ＋
  ★ ✎ ← → ...). They are drawn here on the font's own pixel grid (100 units =
  1 px, 16 px em, cap height 9 px), so they look like the rest of the font
  instead of falling back to a smooth system font. CC0 allows this.

DOS Midnight: PxPlus IBM VGA8 (CC BY-SA 4.0, by VileR), tools/fonts-src/
  pxplus-ibm-vga8. The font is only converted to WOFF2, never changed. The ten
  symbols it lacks go into a separate small font, "GC VGA Symbols", drawn on
  the VGA 8x16 cell (8 px advance, cap height 10 px); CSS lists it right after
  the VGA font, so the browser takes just those glyphs from it.

Classic font choices (Settings > Appearance), all from their authors' files:
  Dogica Pixel (OFL 1.1, Roberto Mocci) - the TTFs are copied unchanged (the
    name "Dogica" is a Reserved Font Name, so the font is never altered). It
    lacks Czech letters (they come from Pixel Operator 8, same 8 px grid) and
    the interface symbols, which "GC Pixel Symbols 8" draws on the 8 px grid.
  Geist Pixel (OFL 1.1, no reserved name) - its variable font is cut down to
    the Regular style and converted to WOFF2 (3.6 MB -> ~26 kB).
  Roboto Slab (Apache 2.0) - converted to WOFF2, weights 100-900 kept.

Writes the web fonts and licence notes to ui/fonts/ (needs fonttools + brotli).
Usage: python3 tools/make_fonts.py tools/fonts-src ui/fonts
"""

import sys
from pathlib import Path

from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.ttLib import TTFont

PX = 100

# (codepoint, baseline offset in px, rows from top to bottom; '#' = pixel)
GLYPHS = [
    (0xFF0B, 1, ["...#...", "...#...", "...#...", "#######", "...#...", "...#...", "...#..."]),  # ＋
    (0x2212, 4, ["#######"]),  # −
    (0x2715, 1, ["#.....#", ".#...#.", "..#.#..", "...#...", "..#.#..", ".#...#.", "#.....#"]),  # ✕
    (0x25B6, 0, ["#....", "##...", "###..", "####.", "#####", "####.", "###..", "##...", "#...."]),  # ▶
    (0x25BA, 0, ["#....", "##...", "###..", "####.", "#####", "####.", "###..", "##...", "#...."]),  # ►
    (0x25C4, 0, ["....#", "...##", "..###", ".####", "#####", ".####", "..###", "...##", "....#"]),  # ◄
    (0x25B8, 2, ["#..", "##.", "###", "##.", "#.."]),  # ▸
    (0x2192, 2, ["....#..", ".....#.", "#######", ".....#.", "....#.."]),  # →
    (0x2190, 2, ["..#....", ".#.....", "#######", ".#.....", "..#...."]),  # ←
    (0x2605, 0, ["....#....", "....#....", "...###...", "#########", ".#######.", "..#####..", "..##.##..", ".##...##.", ".#.....#."]),  # ★
    (0x270E, 0, ["......##", ".....#.#", "....#.#.", "...#.#..", "..#.#...", ".#.#....", "#.#.....", "##......"]),  # ✎
    (0x25A6, 1, ["#######", "#.#.#.#", "#######", "#.#.#.#", "#######", "#.#.#.#", "#######"]),  # ▦
    (0x2630, 1, ["#######", ".......", ".......", "#######", ".......", ".......", "#######"]),  # ☰
    (0x21BB, 0, ["..###.#", ".#...##", "#...###", "#......", "#......", "#.....#", ".#...#.", "..###.."]),  # ↻
    (0x275D, 5, [".#..#.", "#..#..", "##.##.", "##.##."]),  # ❝
    (0x2588, -3, ["#######"] * 16),  # █ (full line height)
]


# Symbols for the VGA font: 8 px cell, glyphs 7 px wide from column 0 (like
# the VGA glyphs themselves), baseline offset in px, rows from top to bottom.
VGA_SYMBOLS = [
    (0x25B6, 0, ["#.....", "##....", "###...", "####..", "#####.", "######", "#####.", "####..", "###...", "##....", "#....."]),  # ▶
    (0x25B8, 2, ["#..", "##.", "###", "##.", "#.."]),  # ▸
    (0x2715, 1, ["##...##", ".##.##.", "..###..", "...#...", "..###..", ".##.##.", "##...##"]),  # ✕
    (0xFF0B, 1, ["..##..", "..##..", "..##..", "######", "..##..", "..##..", "..##.."]),  # ＋
    (0x2605, 0, ["...#...", "...#...", "..###..", "#######", ".#####.", "..###..", ".##.##.", "##...##"]),  # ★
    (0x270E, 0, [".....##", "....###", "...###.", "..###..", ".###...", "###....", "##.....", "#......"]),  # ✎
    (0x21BB, 0, ["..###.#", ".#...##", "#...###", "#......", "#......", "#.....#", ".#...#.", "..###.."]),  # ↻
    (0x275D, 6, ["..#..#", ".#..#.", "##.##.", "##.##."]),  # ❝
    (0x25A6, 1, ["#######", "#.#.#.#", "#######", "#.#.#.#", "#######", "#.#.#.#", "#######"]),  # ▦
    (0x2630, 1, ["#######", "#######", ".......", "#######", "#######", ".......", "#######", "#######"]),  # ☰
]


def draw(rows, offset, lsb=1):
    """A glyph from pixel rows: one rectangle per horizontal run."""
    pen = TTGlyphPen(None)
    height = len(rows)
    for r, row in enumerate(rows):
        y0 = (offset + height - 1 - r) * PX
        x = 0
        while x < len(row):
            if row[x] != "#":
                x += 1
                continue
            start = x
            while x < len(row) and row[x] == "#":
                x += 1
            x0, x1 = (start + lsb) * PX, (x + lsb) * PX
            pen.moveTo((x0, y0))
            pen.lineTo((x0, y0 + PX))
            pen.lineTo((x1, y0 + PX))
            pen.lineTo((x1, y0))
            pen.closePath()
    width = max(len(r) for r in rows)
    return pen.glyph(), (width + 2) * PX


def add_symbols(font):
    cmap_tables = [t for t in font["cmap"].tables if t.isUnicode()]
    order = font.getGlyphOrder()
    for cp, offset, rows in GLYPHS:
        if cp in font.getBestCmap():
            continue
        name = f"uni{cp:04X}"
        glyph, advance = draw(rows, offset)
        font["glyf"][name] = glyph  # also adds the name to the glyph order
        font["hmtx"][name] = (advance, PX)
        if name not in order:
            order.append(name)
        for t in cmap_tables:
            if t.format in (4, 12) or t.platformID == 0:
                if cp <= 0xFFFF or t.format == 12:
                    t.cmap[cp] = name
    font.setGlyphOrder(order)
    font["maxp"].numGlyphs = len(order)


# Symbols on an 8 px grid for Dogica (cap height 7 px), 1 px apart.
PIXEL8_SYMBOLS = [
    (0x25B6, 0, ["#...", "##..", "###.", "####", "###.", "##..", "#..."]),  # ▶
    (0x25BA, 1, ["#..", "##.", "###", "##.", "#.."]),  # ►
    (0x25C4, 1, ["..#", ".##", "###", ".##", "..#"]),  # ◄
    (0x25B8, 1, ["#..", "##.", "###", "##.", "#.."]),  # ▸
    (0x2715, 1, ["#...#", ".#.#.", "..#..", ".#.#.", "#...#"]),  # ✕
    (0xFF0B, 1, ["..#..", "..#..", "#####", "..#..", "..#.."]),  # ＋
    (0x2605, 1, ["..#..", ".###.", "#####", ".###.", ".#.#."]),  # ★
    (0x270E, 1, ["...##", "..###", ".###.", "###..", "#...."]),  # ✎
    (0x21BB, 1, [".##.#", "#..##", "#....", "#...#", ".###."]),  # ↻
    (0x275D, 5, ["#.#", "#.#"]),  # ❝
    (0x25A6, 1, ["#####", "#.#.#", "#####", "#.#.#", "#####"]),  # ▦
    (0x2630, 1, ["#####", ".....", "#####", ".....", "#####"]),  # ☰
    (0x2190, 2, [".#...", "#####", ".#..."]),  # ←
    (0x2192, 2, ["...#.", "#####", "...#."]),  # →
    (0x2212, 3, ["#####"]),  # −
]


def build_symbol_font(target: Path, family: str, symbols, em_px: int, ascent_px: int, descent_px: int,
                      mono_px: int | None, lsb: int):
    """A small font with interface symbols drawn on a pixel grid (100 units =
    1 px). `mono_px`: fixed advance (VGA cell), otherwise width + 2 px."""
    order = [".notdef"] + [f"uni{cp:04X}" for cp, _, _ in symbols]
    glyphs = {".notdef": TTGlyphPen(None).glyph()}
    metrics = {".notdef": ((mono_px or 4) * PX, 0)}
    for cp, offset, rows in symbols:
        name = f"uni{cp:04X}"
        glyphs[name], natural = draw(rows, offset, lsb=lsb)  # natural advance: width + 2 px
        xmin = min(r.index("#") for r in rows if "#" in r)
        advance = mono_px * PX if mono_px else natural
        metrics[name] = (advance, (xmin + lsb) * PX)
    fb = FontBuilder(em_px * PX, isTTF=True)
    fb.setupGlyphOrder(order)
    fb.setupCharacterMap({cp: f"uni{cp:04X}" for cp, _, _ in symbols})
    fb.setupGlyf(glyphs)
    fb.setupHorizontalMetrics(metrics)
    fb.setupHorizontalHeader(ascent=ascent_px * PX, descent=-descent_px * PX)
    fb.setupNameTable({"familyName": family, "styleName": "Regular",
                       "copyright": "Game Curator interface symbols, CC0 1.0 (public domain)"})
    fb.setupOS2(sTypoAscender=ascent_px * PX, sTypoDescender=-descent_px * PX, sTypoLineGap=0,
                usWinAscent=ascent_px * PX, usWinDescent=descent_px * PX, fsType=0)
    fb.setupPost()
    fb.font.flavor = "woff2"
    fb.save(target)
    print(f"{target.name}: {target.stat().st_size} bytes")


def build_vga_symbols(target: Path):
    """GC VGA Symbols: the interface symbols missing in PxPlus IBM VGA8, on the
    same grid (1600 units = 16 px em, ascent 12 px, descent 4 px)."""
    build_symbol_font(target, "GC VGA Symbols", VGA_SYMBOLS, 16, 12, 4, mono_px=8, lsb=0)


def build_classic_choices(src_root: Path, out: Path):
    """The fonts offered for the Classic look besides Pixel Operator."""
    import shutil
    from fontTools.varLib import instancer
    dog = src_root / "dogica"
    for name in ("dogicapixel.ttf", "dogicapixelbold.ttf", "dogica.ttf"):
        shutil.copyfile(dog / name, out / name)  # unchanged (Reserved Font Name)
    (out / "LICENSE-Dogica.txt").write_text(
        (dog / "dogica_pixel_license.txt").read_text(encoding="utf-8-sig") + "\n\n"
        + (dog / "dogica_license.txt").read_text(encoding="utf-8-sig"), encoding="utf-8")
    build_symbol_font(out / "gc-pixel-symbols-8.woff2", "GC Pixel Symbols 8", PIXEL8_SYMBOLS, 8, 7, 1, mono_px=None, lsb=1)

    geist = TTFont(src_root / "geist-pixel" / "GeistPixel-Regular-VariableFont_ELSH.ttf")
    geist = instancer.instantiateVariableFont(geist, {"ELSH": 0})
    geist.flavor = "woff2"
    geist.save(out / "geist-pixel.woff2")
    shutil.copyfile(src_root / "geist-pixel" / "OFL.txt", out / "LICENSE-GeistPixel.txt")

    slab = TTFont(src_root / "roboto-slab" / "RobotoSlab-VariableFont_wght.ttf")
    slab.flavor = "woff2"
    slab.save(out / "roboto-slab.woff2")
    shutil.copyfile(src_root / "roboto-slab" / "LICENSE.txt", out / "LICENSE-RobotoSlab.txt")
    for n in ("geist-pixel.woff2", "roboto-slab.woff2"):
        print(f"{n}: {(out / n).stat().st_size} bytes")


def main(src_root: Path, out: Path):
    out.mkdir(parents=True, exist_ok=True)
    src = src_root / "pixel-operator"
    jobs = {
        "PixelOperator.ttf": ("pixel-operator.woff2", True),
        "PixelOperator-Bold.ttf": ("pixel-operator-bold.woff2", True),
        "PixelOperatorMono.ttf": ("pixel-operator-mono.woff2", False),
        "PixelOperator8.ttf": ("pixel-operator-8.woff2", False),
        "PixelOperator8-Bold.ttf": ("pixel-operator-8-bold.woff2", False),
    }
    for name, (target, symbols) in jobs.items():
        font = TTFont(src / name)
        if symbols:
            add_symbols(font)
        font.flavor = "woff2"
        font.save(out / target)
        print(f"{target}: {(out / target).stat().st_size} bytes")
    (out / "LICENSE-PixelOperator.txt").write_text(
        "Pixel Operator by Jayvee Enaguas (HarvettFox96), CC0 1.0 Universal (public domain).\n"
        "Game Curator adds pixel glyphs for a few interface symbols (see tools/make_fonts.py).\n\n"
        + (src / "LICENSE.txt").read_text(encoding="utf-8", errors="replace")
    )

    # DOS Midnight: the VGA font converted to WOFF2 as it is, plus the symbols
    vga = src_root / "pxplus-ibm-vga8"
    font = TTFont(vga / "PxPlus_IBM_VGA8.ttf")
    font.flavor = "woff2"
    font.save(out / "pxplus-ibm-vga8.woff2")
    print(f"pxplus-ibm-vga8.woff2: {(out / 'pxplus-ibm-vga8.woff2').stat().st_size} bytes")
    (out / "LICENSE-PxPlus-IBM-VGA8.txt").write_text((vga / "LICENSE.txt").read_text(encoding="utf-8"))
    build_vga_symbols(out / "gc-vga-symbols.woff2")
    build_classic_choices(src_root, out)


if __name__ == "__main__":
    main(Path(sys.argv[1]), Path(sys.argv[2]))
