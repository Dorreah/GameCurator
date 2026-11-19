"""Game Curator icons, in the violet palette of the default theme.

* The application icon: three tall big-box game boxes standing on a shelf
  like books in a library; the last one is open (a nod to the boxed PC games
  of the 1990s and to opening the archive again).
* A single open box, used in the UI as the placeholder of games without
  artwork.

Writes
  <icons>/icon.png, <icons>/icon.ico   application icon (exe, window, taskbar)
  <ui>/img/shelf.png                    the icon for the UI (logo, About)
  <ui>/img/box.svg                      the open box placeholder
Both UI images are single-colour; the shades are alpha levels, so CSS
recolours them with `mask` per theme. In box.svg the shapes only touch at
their edges, so the alpha levels stay exact; the shelf is a PNG because its
shapes overlap (painter's order) and the outlines become transparent gaps.
Usage: python3 tools/make_icons.py src-tauri/icons ui/img
"""
import sys
from pathlib import Path

from PIL import Image, ImageDraw

# Geometry in a 512x512 box. Shapes only touch at their edges (no overlap),
# so the alpha levels of the SVG stay exact when used as a CSS mask.
T, R, B, L = (256, 150), (416, 230), (256, 310), (96, 230)
B2, R2, L2 = (256, 470), (416, 390), (96, 390)
BOX = [
    # name, points, colour (icon), alpha (mask svg)
    ("back-left flap", [L, T, (226, 62), (66, 142)], "#6b4a9e", 0.45),
    ("back-right flap", [T, R, (446, 142), (286, 62)], "#5e418c", 0.38),
    ("inside", [T, R, B, L], "#1d1529", 0.10),
    ("left side", [(96, 302), (196, 352), B, B2, L2], "#7f58ba", 0.82),
    ("right side", [B, (316, 352), (416, 302), R2, B2], "#5c3f8a", 0.58),
    ("front-left flap", [L, B, (196, 352), (36, 272)], "#a17cdc", 1.0),
    ("front-right flap", [B, R, (476, 272), (316, 352)], "#8d68c9", 0.74),
]



# ---- the shelf: oblique view, depth vector (34, -24). Drawn back to front
# (painter's order), so later shapes may cover earlier ones.
def rect(x0, y0, x1, y1):
    return [(x0, y0), (x1, y0), (x1, y1), (x0, y1)]


def box(x0, x1, top, band, colours, alphas, bottom=420):
    """One upright box seen from the front: right side, top, front with a band."""
    c_side, c_top, c_front, c_band = colours
    a_side, a_top, a_front, a_band = alphas
    b0, b1 = band
    return [
        ("side", [(x1, top), (x1 + 34, top - 24), (x1 + 34, bottom - 24), (x1, bottom)], c_side, a_side),
        ("top", [(x0, top), (x1, top), (x1 + 34, top - 24), (x0 + 34, top - 24)], c_top, a_top),
        ("front", rect(x0, top, x1, bottom), c_front, a_front),
        ("band", rect(x0, b0, x1, b1), c_band, a_band),
    ]


SHELF_RAW = [
    ("shelf top", [(40, 420), (446, 420), (480, 396), (74, 396)], "#3d2f57", 0.40),
    ("shelf side", [(446, 420), (480, 396), (480, 426), (446, 450)], "#2b2140", 0.30),
    *box(70, 162, 104, (146, 172), ("#4a3370", "#b598e6", "#8d68c9", "#5e418c"), (0.34, 0.72, 0.90, 0.50)),
    *box(162, 262, 150, (190, 214), ("#4a3370", "#c6adf0", "#a17cdc", "#6b4a9e"), (0.34, 0.80, 1.0, 0.56)),
    # box 3: open, flaps up
    ("box3 back flap", [(312, 202), (418, 202), (440, 128), (334, 128)], "#6b4a9e", 0.50),
    ("box3 side", [(384, 226), (418, 202), (418, 396), (384, 420)], "#3f2b60", 0.30),
    ("box3 inside", [(278, 226), (384, 226), (418, 202), (312, 202)], "#1d1529", 0.08),
    ("box3 right flap", [(384, 226), (418, 202), (476, 198), (442, 222)], "#5e418c", 0.42),
    ("box3 front", rect(278, 226, 384, 420), "#7f58ba", 0.80),
    ("box3 band", rect(278, 296, 384, 318), "#4f3678", 0.44),
    ("box3 front flap", [(278, 226), (384, 226), (352, 270), (246, 270)], "#c6adf0", 0.92),
    ("shelf front", rect(40, 420, 446, 450), "#5a4580", 0.62),
]


def _fit(shapes, k=1.06, cx=260, cy=278):
    return [(n, [((x - cx) * k + 256, (y - cy) * k + 256) for x, y in pts], c, a) for n, pts, c, a in shapes]


SHELF = _fit(SHELF_RAW)


def draw_png(shapes, size: int, outline: int = 7) -> Image.Image:
    s = 4  # supersampling
    img = Image.new("RGBA", (512 * s, 512 * s), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    for _, pts, colour, _ in shapes:
        d.polygon([(x * s, y * s) for x, y in pts], fill=colour, outline="#1a1226", width=outline * s)
    return img.resize((size, size), Image.LANCZOS)


def write_mask_png(shapes, path: Path, size: int = 256, outline: int = 9):
    """White image whose alpha carries the shades; outlines become gaps."""
    s = 4
    alpha = Image.new("L", (512 * s, 512 * s), 0)
    d = ImageDraw.Draw(alpha)
    for _, pts, _, a in shapes:
        d.polygon([(x * s, y * s) for x, y in pts], fill=int(a * 255), outline=0, width=outline * s)
    x0, y0, x1, y1 = alpha.getbbox()
    side = max(x1 - x0, y1 - y0)
    square = Image.new("L", (side, side), 0)
    square.paste(alpha.crop((x0, y0, x1, y1)), ((side - (x1 - x0)) // 2, (side - (y1 - y0)) // 2))
    alpha = square.resize((size, size), Image.LANCZOS)
    img = Image.new("RGBA", (size, size), (255, 255, 255, 0))
    img.putalpha(alpha)
    img.save(path, optimize=True)


def write_svg(shapes, path: Path, view_box: str):
    polys = "".join(
        f'<polygon points="{" ".join(f"{x},{y}" for x, y in pts)}" fill-opacity="{a}"/>' for _, pts, _, a in shapes
    )
    path.write_text(f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="{view_box}">{polys}</svg>\n')


icons, ui = Path(sys.argv[1]), Path(sys.argv[2])
icons.mkdir(parents=True, exist_ok=True)
big = draw_png(SHELF, 512)
big.save(icons / "icon.png")
# small sizes get a thinner outline so the three boxes stay separate
small = {n: draw_png(SHELF, n, outline=12 if n <= 32 else 9) for n in (16, 24, 32, 48)}
big.save(icons / "icon.ico", sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)],
         append_images=list(small.values()))
write_mask_png(SHELF, ui / "shelf.png")
write_svg(BOX, ui / "box.svg", "30 50 452 430")
draw_png(SHELF, 128).save("/tmp/icon-128.png")
for n, im in small.items():
    im.resize((n * 4, n * 4), Image.NEAREST).save(f"/tmp/icon-{n}x4.png")
print("written:", icons / "icon.png", icons / "icon.ico", ui / "shelf.png", ui / "box.svg")
