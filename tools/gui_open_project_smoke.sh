#!/usr/bin/env bash
# Native X11 project-open keyboard flow; not a cross-platform desktop audit.
set -eu
work=$1
gui=$2
cli=$3
close_helper=$4
mkdir "$work"
cd "$work"
pid=
cleanup() {
    if [ -n "$pid" ]; then kill "$pid" 2>/dev/null || true; wait "$pid" 2>/dev/null || true; fi
}
trap cleanup EXIT
export LIBGL_ALWAYS_SOFTWARE=1 WINIT_X11_SCALE_FACTOR=1
"$cli" new 32 32 source.json
"$cli" new 5 5 other.json
cp source.json source-original.json
cp other.json other-original.json
"$gui" "$work/source.json" > "$work/gui.log" 2>&1 &
pid=$!
window=$(timeout 30s xdotool search --sync --onlyvisible --name Perveance | head -n 1)
kill -0 "$pid"
sleep 2
xdotool windowfocus --sync "$window"
# Make the source dirty. Choosing another file must not silently discard it.
xdotool mousemove --window "$window" 438 112 click 1
sleep 1
choose_other() {
    xdotool key --window "$window" ctrl+o
    sleep 1
    xdotool key --window "$window" ctrl+a
    xdotool type --window "$window" --clearmodifiers 'other.json'
    xdotool key --window "$window" Return
    sleep 1
}
choose_other
# Escape the discard warning; saving now must retain the edit on the source.
xdotool key --window "$window" Escape
sleep 1
xdotool key --window "$window" ctrl+s
sleep 1
test -f source.v001.json || { echo 'Cancel/open did not preserve source edit'; exit 1; }
echo 'Dirty-open confirmation cancelled; source version saved.'
# Once saved, opening another project needs no confirmation.
choose_other
sleep 1
# Draw on the centered 5x5 canvas and save its first version.
xdotool mousemove --window "$window" 630 112 click 1
sleep 1
xdotool key --window "$window" ctrl+s
sleep 1
test -f other.v001.json || { echo 'Other project was not opened and version-saved'; exit 1; }
cmp source.json source-original.json
cmp other.json other-original.json
python3 - <<'PY'
import json
from pathlib import Path
for name, size in [('source.v001.json', (32, 32)), ('other.v001.json', (5, 5))]:
    p = json.loads(Path(name).read_text())
    assert (p['width'], p['height']) == size, (name, p['width'], p['height'])
    assert sum(pixel[3] > 0 for pixel in p['layers'][0]['pixels']) == 1, name
PY
/usr/bin/python3 "$close_helper" "$window"
sleep 2
if kill -0 "$pid" 2>/dev/null; then echo 'Saved project did not close cleanly'; exit 1; fi
wait "$pid"
pid=
echo 'Native Ctrl+O, cancel, open and save workflow passed.'
