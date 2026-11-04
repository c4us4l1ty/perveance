#!/usr/bin/env bash
# Runs under a fixed-size virtual X11 display with a trusted built binary.
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
start_gui() {
    "$gui" "$@" > "$work/gui.log" 2>&1 &
    pid=$!
    window=$(timeout 30s xdotool search --sync --onlyvisible --name Perveance | head -n 1)
    kill -0 "$pid"
    sleep 2
    xdotool windowfocus --sync "$window"
}
save() {
    xdotool key --window "$window" ctrl+s
    sleep 1
}
close_clean() {
    /usr/bin/python3 "$close_helper" "$window"
    sleep 2
    if kill -0 "$pid" 2>/dev/null; then echo 'Saved project did not close cleanly'; exit 1; fi
    wait "$pid"
    pid=
}
export LIBGL_ALWAYS_SOFTWARE=1 WINIT_X11_SCALE_FACTOR=1
start_gui
xdotool mousemove --window "$window" 438 112 click 1
sleep 1
save
test -f art.perveance.json
xdotool mousemove --window "$window" 462 136 click 1
sleep 1
save
test -f art.perveance.v001.json
close_clean
start_gui "$work/art.perveance.v001.json"
xdotool mousemove --window "$window" 486 160 click 1
sleep 1
save
test -f art.perveance.v002.json
save
test ! -e art.perveance.v003.json
close_clean
python3 - <<'PY'
import json
from pathlib import Path
files = ["art.perveance.json", "art.perveance.v001.json", "art.perveance.v002.json"]
projects = [json.loads(Path(file).read_text()) for file in files]
for index, project in enumerate(projects):
    assert (project["width"], project["height"]) == (32, 32)
    assert len(project["layers"]) == 1
    colored = sum(pixel[3] > 0 for pixel in project["layers"][0]["pixels"])
    assert colored == index + 1, (files[index], colored)
PY
if "$gui" "$work/missing.json" > "$work/invalid.log" 2>&1; then
    echo 'Missing project unexpectedly opened'; exit 1
fi
grep -q 'Could not open project' "$work/invalid.log"
echo 'Native GUI save, reopen and versioned editing round trip passed.'
