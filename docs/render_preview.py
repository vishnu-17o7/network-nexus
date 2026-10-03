"""Render NEXUS --render output as a terminal grid (requires Pillow)."""
import json
import re
import sys
from PIL import Image, ImageDraw, ImageFont

buffer = json.load(open(sys.argv[1]))
regular = ImageFont.truetype("/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf", 17)
bold = ImageFont.truetype("/usr/share/fonts/truetype/dejavu/DejaVuSansMono-Bold.ttf", 17)
cell_w, cell_h, margin = regular.getlength("M"), 23, 22
fallback = ImageFont.truetype("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf", 17)
image = Image.new("RGB", (round(buffer["width"] * cell_w + margin * 2), buffer["height"] * cell_h + margin * 2), (13, 18, 29))
draw = ImageDraw.Draw(image)

def color(value, fallback):
    numbers = re.match(r"Rgb\((\d+), (\d+), (\d+)\)", value)
    if numbers:
        return tuple(map(int, numbers.groups()))
    return {"Black": (0, 0, 0), "White": (255, 255, 255)}.get(value, fallback)

for cell in buffer["cells"]:
    x, y = margin + cell["x"] * cell_w, margin + cell["y"] * cell_h
    draw.rectangle((x, y, x + cell_w - 1, y + cell_h - 1), fill=color(cell["bg"], (13, 18, 29)))
    draw.text((x, y + 1), cell["symbol"], font=fallback if any(0x2800 <= ord(c) <= 0x28FF for c in cell["symbol"]) else (bold if cell["bold"] else regular), fill=color(cell["fg"], (226, 234, 248)))
image.save(sys.argv[2])
