"""Check real editor input on the fixed-size CI X11 display."""
from PIL import Image


def pixel(path, xy):
    with Image.open(path) as image:
        return image.convert("RGB").getpixel(xy)


def is_ink(color):
    return color[0] > 120 and color[1] > 150 and color[2] < 110


border = pixel("/tmp/perveance-shape.png", (438, 112))
line = pixel("/tmp/perveance-line.png", (534, 208))
cleared = pixel("/tmp/perveance-eyedropper.png", (486, 112))
print("Rectangle border:", border, "line:", line, "sampled-transparent pencil:", cleared)
assert is_ink(border), border
assert is_ink(line), line
assert not is_ink(cleared), cleared
