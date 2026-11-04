#!/usr/bin/env bash
# Native X11 smoke for tool selection, undo/redo and modal shortcut suppression.
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
export LIBGL_ALWAYS_SOFTWARE=1 WINIT_X11_SCALE_FACTOR=1
"$gui" > "$work/gui.log" 2>&1 &
pid=$!
window=$(timeout 30s xdotool search --sync --onlyvisible --name Perveance | head -n 1)
kill -0 "$pid"
sleep 2
xdotool windowfocus --sync "$window"
# Select Line using L and commit a 5-pixel diagonal.
xdotool key --window "$window" l
xdotool mousemove --window "$window" 438 112 mousedown 1
sleep 0.3
xdotool mousemove --sync --window "$window" 486 160
sleep 0.3
xdotool mouseup 1
sleep 1
xdotool key --window "$window" ctrl+s
sleep 1
test -f art.perveance.json || { echo 'Initial save missing'; exit 1; }
# B selects Pencil; undo and redo its single-pixel edit.
xdotool key --window "$window" b
xdotool mousemove --window "$window" 558 208 click 1
sleep 1
xdotool key --window "$window" ctrl+z
sleep 0.3
xdotool key --window "$window" ctrl+shift+z
sleep 1
xdotool key --window "$window" ctrl+s
sleep 1
test -f art.perveance.v001.json || { echo 'Version after redo missing'; exit 1; }
# Ctrl+O opens the dialog rather than selecting Ellipse. Typing E while the
# filename is focused must not switch the selected tool to Eraser.
xdotool key --window "$window" ctrl+o
sleep 1
xdotool key --window "$window" e
xdotool key --window "$window" Escape
sleep 1
xdotool mousemove --window "$window" 582 232 click 1
sleep 1
xdotool key --window "$window" ctrl+s
sleep 1
test -f art.perveance.v002.json || { echo 'Version after modal cancel missing'; exit 1; }
python3 - <<'PY'
import json
from pathlib import Path
for name, expected in [('art.perveance.json', 5), ('art.perveance.v001.json', 6), ('art.perveance.v002.json', 7)]:
    project = json.loads(Path(name).read_text())
    pixels = project['layers'][0]['pixels']
    assert sum(pixel[3] > 0 for pixel in pixels) == expected, (name, expected)
PY
/usr/bin/python3 "$close_helper" "$window"
sleep 2
if kill -0 "$pid" 2>/dev/null; then echo 'Saved project did not close cleanly'; exit 1; fi
wait "$pid"
pid=
echo 'Native tool shortcuts, undo/redo and dialog isolation passed.'
