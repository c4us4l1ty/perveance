#!/usr/bin/env bash
# Fixed-layout native X11 test; not a real-desktop or accessibility audit.
set -eu
work=$1
gui=$2
close_helper=$3
mkdir "$work"
cd "$work"
pid=
cleanup() {
    if [ -n "$pid" ]; then kill "$pid" 2>/dev/null || true; wait "$pid" 2>/dev/null || true; fi
}
trap cleanup EXIT
python3 - <<'PY'
import json
from pathlib import Path
pixels = [[x + 1, y + 1, 33, [0, 128, 255][(x + y) % 3]] for y in range(32) for x in range(32)]
project = {'version': 1, 'width': 32, 'height': 32, 'layers': [
    {'name': 'Raw RGBA', 'visible': True, 'pixels': pixels},
    {'name': 'Hidden sentinel', 'visible': False, 'pixels': [[99, 88, 77, 0] for _ in range(1024)]}]}
Path('region.perveance.json').write_text(json.dumps(project))
PY
cp region.perveance.json original-copy.json
export LIBGL_ALWAYS_SOFTWARE=1 WINIT_X11_SCALE_FACTOR=1
"$gui" "$work/region.perveance.json" > "$work/gui.log" 2>&1 &
pid=$!
window=$(timeout 30s xdotool search --sync --onlyvisible --name Perveance | head -n 1)
kill -0 "$pid"
sleep 2
xdotool windowfocus --sync "$window"
save() { xdotool key --window "$window" ctrl+s; sleep 1; }
select_source() {
    # Reverse drag: inclusive bounds should yield exactly three by two pixels.
    xdotool mousemove --window "$window" 462 124 mousedown 1
    sleep 0.3
    xdotool mousemove --sync --window "$window" 438 112
    sleep 0.3
    xdotool mouseup 1
    sleep 0.5
}
xdotool key --window "$window" m
sleep 0.5
select_source
xdotool key --window "$window" c
sleep 0.5
save
test ! -e region.perveance.v001.json || { echo 'Selection/copy unexpectedly modified artwork'; exit 1; }
# Click a destination ten pixels right and eight down from the source origin.
xdotool mousemove --window "$window" 558 208 click 1
sleep 0.5
xdotool key --window "$window" v
sleep 0.5
save
test -f region.perveance.v001.json || { echo 'Region paste was not saved'; exit 1; }
xdotool key --window "$window" x
sleep 0.5
save
test -f region.perveance.v002.json || { echo 'Region cut was not saved'; exit 1; }
# Undo cut, copy the full canvas, then try to paste it at a nonzero position.
# Rejection and copy/selection must preserve redo, so redo returns to the cut.
xdotool key --window "$window" ctrl+z
sleep 0.5
xdotool key --window "$window" ctrl+a
sleep 0.3
xdotool key --window "$window" c
sleep 0.3
xdotool mousemove --window "$window" 438 112 click 1
sleep 0.5
xdotool key --window "$window" v
sleep 0.5
xdotool key --window "$window" ctrl+y
sleep 0.5
save
test ! -e region.perveance.v003.json || { echo 'Failed paste lost redo or changed artwork'; exit 1; }
# Region shortcuts must not affect artwork while typing in the open browser.
xdotool key --window "$window" ctrl+o
sleep 1
xdotool key --window "$window" x v Delete
xdotool key --window "$window" Escape
sleep 0.5
save
test ! -e region.perveance.v003.json || { echo 'Browser typing changed artwork'; exit 1; }
select_source
xdotool key --window "$window" Delete
sleep 0.5
save
test -f region.perveance.v003.json || { echo 'Region clear was not saved'; exit 1; }
xdotool key --window "$window" ctrl+z
sleep 0.5
save
test -f region.perveance.v004.json || { echo 'Undo of region clear was not saved'; exit 1; }
/usr/bin/python3 "$close_helper" "$window"
sleep 2
if kill -0 "$pid" 2>/dev/null; then echo 'Saved region project did not close cleanly'; exit 1; fi
wait "$pid"
pid=
cmp region.perveance.json original-copy.json
python3 - <<'PY'
import copy
import json
from pathlib import Path
projects = [json.loads(Path(name).read_text()) for name in ['region.perveance.json'] +
            [f'region.perveance.v{i:03}.json' for i in range(1, 5)]]
before, pasted, cut, cleared, undone = projects
changes = [i for i, (a, b) in enumerate(zip(before['layers'][0]['pixels'], pasted['layers'][0]['pixels'])) if a != b]
assert len(changes) == 6, changes
xs, ys = {i % 32 for i in changes}, {i // 32 for i in changes}
tx, ty = min(xs), min(ys)
assert xs == set(range(tx, tx + 3)) and ys == set(range(ty, ty + 2)), (xs, ys)
sample = pasted['layers'][0]['pixels'][ty * 32 + tx]
sx, sy = sample[0] - 1, sample[1] - 1
assert (tx - sx, ty - sy) == (10, 8), (tx, ty, sx, sy)
expected = copy.deepcopy(before)
for row in range(2):
    for column in range(3):
        expected['layers'][0]['pixels'][(ty + row) * 32 + tx + column] = before['layers'][0]['pixels'][(sy + row) * 32 + sx + column]
assert pasted == expected, 'Paste must replace exact raw bytes, including transparency'
for row in range(2):
    for column in range(3):
        expected['layers'][0]['pixels'][(ty + row) * 32 + tx + column] = [0, 0, 0, 0]
assert cut == expected and undone == expected, 'Cut and undo/redo must be exact'
for row in range(2):
    for column in range(3):
        expected['layers'][0]['pixels'][(sy + row) * 32 + sx + column] = [0, 0, 0, 0]
assert cleared == expected, 'Clear must affect only the selected source region'
assert all(p['layers'][1] == before['layers'][1] for p in projects)
PY
echo 'Native region selection, copy/cut/paste/clear, undo/redo, bounds rejection and dialog isolation passed.'
