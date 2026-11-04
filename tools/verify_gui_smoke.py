"""Check real editor input on the fixed-size CI X11 display."""
from PIL import Image


def pixel(path, xy):
    with Image.open(path) as image:
        return image.convert("RGB").getpixel(xy)


def is_ink(color):
    return color[0] > 120 and color[1] > 150 and color[2] < 110


border = pixel("/tmp/perveance-shape.png", (438, 112))
before_clear = pixel("/tmp/perveance-line.png", (486, 112))
# Away from the thin preview guide: still checkerboard while the drag is held.
line_preview = pixel("/tmp/perveance-line-preview.png", (531, 208))
line_committed = pixel("/tmp/perveance-line.png", (531, 208))
cleared = pixel("/tmp/perveance-eyedropper.png", (486, 112))
print("Rectangle border:", border, "before clear:", before_clear,
      "line preview:", line_preview, "line committed:", line_committed,
      "sampled-transparent pencil:", cleared)
assert is_ink(border), border
assert is_ink(before_clear), before_clear
# The preview guide can be anti-aliased over this pixel, so it may be greenish
# without being a committed pixel. A committed stroke must be substantially brighter.
assert line_preview[1] < line_committed[1] - 35, (line_preview, line_committed)
assert is_ink(line_committed), line_committed
assert cleared[0] == cleared[1] == cleared[2] and cleared[0] < 100, cleared

# A duplicated layer is mirrored without moving the original, then cleared and undone.
original = pixel("/tmp/perveance-flipped.png", (438, 112))
flipped = pixel("/tmp/perveance-flipped.png", (762, 112))
cleared_flip = pixel("/tmp/perveance-cleared.png", (762, 112))
original_after_clear = pixel("/tmp/perveance-cleared.png", (438, 112))
restored = pixel("/tmp/perveance-clear-undone.png", (762, 112))
print("Original:", original, "flipped copy:", flipped, "cleared:", cleared_flip, "undo:", restored)
assert is_ink(original), original
assert is_ink(flipped), flipped
assert not is_ink(cleared_flip), cleared_flip
assert is_ink(original_after_clear), original_after_clear
assert is_ink(restored), restored

# The close warning darkens the canvas; Escape keeps the app and artwork open.
warning = pixel("/tmp/perveance-close-warning.png", (438, 112))
after_cancel = pixel("/tmp/perveance-close-cancelled.png", (438, 112))
print("Close warning:", warning, "after cancel:", after_cancel)
assert warning[1] < original[1] - 25, warning
assert is_ink(after_cancel), after_cancel
