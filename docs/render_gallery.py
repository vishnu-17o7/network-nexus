"""Compose screenshots from the actual preview buffers (Pillow + DejaVu fonts).

Usage: python3 docs/render_gallery.py target/ui-previews target/ui-screenshots
No app data is synthesized here. Fixture data is explicitly labeled by the app.
"""
import math
from pathlib import Path
import subprocess
import sys
from PIL import Image, ImageDraw, ImageFont

source, output = map(Path, sys.argv[1:3])
output.mkdir(parents=True, exist_ok=True)
renderer = Path(__file__).with_name("render_preview.py")
for buffer in sorted(source.glob("*.json")):
    subprocess.run([sys.executable, str(renderer), str(buffer), str(output / (buffer.stem + ".png"))], check=True)

def sheet(name, entries, columns=2, width=740):
    images = [(title, Image.open(output / (stem + ".png")).convert("RGB")) for title, stem in entries]
    height = max(round(im.height * width / im.width) for _, im in images)
    canvas = Image.new("RGB", (columns * width, math.ceil(len(images) / columns) * (height + 32)), (13, 18, 26))
    draw = ImageDraw.Draw(canvas)
    font = ImageFont.truetype("/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf", 16)
    for n, (title, im) in enumerate(images):
        x, y = n % columns * width, n // columns * (height + 32)
        draw.text((x + 12, y + 8), title, font=font, fill=(226, 232, 240))
        im.thumbnail((width, height), getattr(Image, "Resampling", Image).LANCZOS)
        canvas.paste(im, (x, y + 32))
    canvas.save(output / name)

sheet("workbench-gallery.png", [("Interfaces", "page-01-wide"), ("Diagnostics", "page-10-wide"), ("Bandwidth", "page-07-wide"), ("Preferences", "page-13-wide"), ("Tailscale", "page-15-wide"), ("Pi-hole", "page-16-wide")])
for size in ["wide", "compact", "tall"]:
    sheet("review-" + size + ".png", [(p.stem, p.stem) for p in sorted(source.glob("page-*-" + size + ".json"))], columns=3, width=600)
sheet("review-empty.png", [(p.stem, p.stem) for p in sorted(source.glob("empty-*.json"))], columns=3, width=600)
print(f"Rendered {len(list(source.glob('*.json')))} real UI buffers and four review sheets in {output}")
