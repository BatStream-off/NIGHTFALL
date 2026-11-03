"""CI: fail when the NIGHTFALL window shows no interface (blank window).

The interface is dark; a blank window is white or light grey. The centre of
the screen (where the window is) must be mostly dark.
"""
import sys
from PIL import Image

im = Image.open(sys.argv[1]).convert("L")
w, h = im.size
box = im.crop((w // 4, h // 4, 3 * w // 4, 3 * h // 4))
hist = box.histogram()
dark = sum(hist[:60]) / sum(hist)
bright = sum(hist[150:]) / sum(hist)
print(f"pixels sombres au centre de l'écran : {dark:.0%}, pixels clairs (texte, icône) : {bright:.2%}")
# Dark interface AND visible content (the lock screen has text and an icon).
if dark < 0.5 or bright < 0.001:
    print("La fenêtre NIGHTFALL n'affiche pas l'interface (fenêtre vide).")
    sys.exit(1)
