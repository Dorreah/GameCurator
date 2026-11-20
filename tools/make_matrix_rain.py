"""Generates ui/img/matrix-rain.svg: a static, tileable "digital rain" texture
for the Matrix theme (no animation). Deterministic (fixed seed)."""
import random
import sys
from pathlib import Path

W = H = 480
STEP_X, STEP_Y = 16, 16
CHARS = "0123456789ABCDEFHKMNRTXZ:=*+<>|"
random.seed(1999)
HEAD, TRAIL = "#eef4f0", "#6cc48f"   # the Matrix theme's white and soft green

parts = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" viewBox="0 0 {W} {H}" '
         'font-family="Consolas,Lucida Console,monospace" font-size="14" font-weight="bold">']
rows = H // STEP_Y
for col in range(W // STEP_X):
    x = col * STEP_X + 3
    # one or two drops per column; the pattern wraps vertically so it tiles
    for _ in range(random.choice([1, 1, 2])):
        head = random.randrange(rows)
        length = random.randint(5, 16)
        for k in range(length):
            row = (head - k) % rows
            y = row * STEP_Y + 13
            ch = random.choice(CHARS)
            if k == 0:
                fill, op = HEAD, 0.85
            else:
                fill, op = TRAIL, round(max(0.08, 0.85 - k / length), 2)
            mirror = random.random() < 0.3
            ch = {"<": "&lt;", ">": "&gt;"}.get(ch, ch)
            if mirror:
                parts.append(f'<text transform="translate({2 * x + 9} 0) scale(-1 1)" x="{x}" y="{y}" fill="{fill}" fill-opacity="{op}">{ch}</text>')
            else:
                parts.append(f'<text x="{x}" y="{y}" fill="{fill}" fill-opacity="{op}">{ch}</text>')
parts.append("</svg>")
out = Path(sys.argv[1])
out.write_text("".join(parts))
print(out, out.stat().st_size, "bytes")
