#!/usr/bin/env bash
# Run under a disposable X11 display. Kill the editor, then reopen its checkpoint.
set -eu
work=$1
gui=$2
close_helper=$3
mkdir -p "$work/checkpoints"
cd "$work"
pid=
cleanup() {
    if [ -n "$pid" ]; then kill "$pid" 2>/dev/null || true; wait "$pid" 2>/dev/null || true; fi
}
trap cleanup EXIT
export LIBGL_ALWAYS_SOFTWARE=1 WINIT_X11_SCALE_FACTOR=1
"$gui" --recovery-dir "$work/checkpoints" > "$work/gui.log" 2>&1 &
pid=$!
window=$(timeout 30s xdotool search --sync --onlyvisible --name Perveance | head -n 1)
kill -0 "$pid"
sleep 2
xdotool mousemove --window "$window" 438 112 click 1
# Allow the real checkpoint timer to fire, then simulate an abrupt process death.
sleep 18
kill -KILL "$pid"
wait "$pid" 2>/dev/null || true
pid=
checkpoint=$(python3 - <<'PY'
import json
from pathlib import Path
paths = list(Path('checkpoints').glob('perveance-recovery-*.perveance.json'))
assert len(paths) == 1, paths
project = json.loads(paths[0].read_text())
assert (project['width'], project['height']) == (32, 32)
assert sum(pixel[3] > 0 for layer in project['layers'] for pixel in layer['pixels']) == 1
print(paths[0].resolve())
PY
)
"$gui" "$checkpoint" > "$work/reopened.log" 2>&1 &
pid=$!
window=$(timeout 30s xdotool search --sync --onlyvisible --name Perveance | head -n 1)
kill -0 "$pid"
sleep 2
scrot "$work/reopened.png"
python3 - "$work/reopened.png" <<'PY'
from PIL import Image
import sys
with Image.open(sys.argv[1]) as image:
    r, g, b = image.convert('RGB').getpixel((438, 112))
assert r > 120 and g > 150 and b < 110, (r, g, b)
PY
/usr/bin/python3 "$close_helper" "$window"
sleep 2
if kill -0 "$pid" 2>/dev/null; then echo 'Recovered project did not close cleanly'; exit 1; fi
wait "$pid"
pid=
echo 'Unsaved artwork survived forced process termination and reopened in the GUI.'
