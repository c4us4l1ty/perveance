"""Emit concise image diagnostics as CI annotations when GUI smoke fails."""
from pathlib import Path
from PIL import Image

for name in ('shape', 'line', 'eyedropper', 'flipped', 'cleared'):
    path = Path('/tmp') / ('perveance-' + name + '.png')
    if not path.exists():
        continue
    with Image.open(path) as image:
        image = image.convert('RGB')
        found = []
        for y in range(80, min(500, image.height)):
            for x in range(200, min(1000, image.width)):
                r, g, b = image.getpixel((x, y))
                if r > 120 and g > 150 and b < 110:
                    found.append((x, y))
        bbox = (min(x for x, y in found), min(y for x, y in found),
                max(x for x, y in found), max(y for x, y in found)) if found else None
        samples = {str(p): image.getpixel(p) for p in [(438, 112), (486, 112), (531, 208), (770, 112)]}
        print(f'::error::{name} ink bbox={bbox}, count={len(found)}, samples={samples}')
