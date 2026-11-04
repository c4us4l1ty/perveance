"""Check the rectangle-drag smoke screenshot from the fixed-size CI X11 display."""
from PIL import Image

image = Image.open("/tmp/perveance-shape.png").convert("RGB")
pixel = image.getpixel((438, 112))
print("Rectangle border pixel:", pixel)
assert pixel[0] > 120 and pixel[1] > 150 and pixel[2] < 110, pixel
