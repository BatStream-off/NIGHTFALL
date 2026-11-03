"""Share of magenta pixels in a screenshot (CI display experiments)."""
import sys
from PIL import Image

im = Image.open(sys.argv[2]).convert("RGB")
r, g, b = im.split()
mag = 0
w, h = im.size
for y in range(0, h, 4):
    for x in range(0, w, 4):
        pr, pg, pb = im.getpixel((x, y))
        if pr > 200 and pb > 200 and pg < 80:
            mag += 1
total = len(range(0, h, 4)) * len(range(0, w, 4))
print(f"EXPERIENCE {sys.argv[1]} : magenta {mag / total:.0%}")
