"""CI: the end of the self test shows https://example.com, a light page.
Fail when the web content area stays dark (web page not displayed)."""
import sys
from PIL import Image

im = Image.open(sys.argv[1]).convert("L")
w, h = im.size
# Lower middle of the screen: inside the web content area of the window.
box = im.crop((w // 4, int(h * 0.45), 3 * w // 4, int(h * 0.75)))
hist = box.histogram()
light = sum(hist[180:]) / sum(hist)
print(f"zone de contenu web claire : {light:.0%}")
if light < 0.4:
    print("Le contenu de la page web n'est pas affiché.")
    sys.exit(1)
