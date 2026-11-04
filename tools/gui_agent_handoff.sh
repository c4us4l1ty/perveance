#!/usr/bin/env bash
# A bounded file handoff, not a live Arena or GUI connector.
set -eu
work=$1
gui=$2
cli=$3
bridge=$4
fixture=$5
close_helper=$6
mkdir "$work"
cd "$work"
pid=
cleanup() {
    if [ -n "$pid" ]; then kill "$pid" 2>/dev/null || true; wait "$pid" 2>/dev/null || true; fi
}
trap cleanup EXIT
export LIBGL_ALWAYS_SOFTWARE=1 WINIT_X11_SCALE_FACTOR=1
python3 "$bridge" create 32 32 "$fixture" rocket.perveance.json > created.json
cp rocket.perveance.json original-copy.json
"$gui" "$work/rocket.perveance.json" > "$work/gui.log" 2>&1 &
pid=$!
window=$(timeout 30s xdotool search --sync --onlyvisible --name Perveance | head -n 1)
kill -0 "$pid"
sleep 2
xdotool windowfocus --sync "$window"
xdotool mousemove --window "$window" 438 112 click 1
sleep 1
xdotool key --window "$window" ctrl+s
sleep 1
test -f rocket.perveance.v001.json
/usr/bin/python3 "$close_helper" "$window"
sleep 2
if kill -0 "$pid" 2>/dev/null; then echo 'GUI project did not close cleanly'; exit 1; fi
wait "$pid"
pid=
cmp rocket.perveance.json original-copy.json
python3 "$bridge" inspect rocket.perveance.v001.json > inspected.json
python3 - <<'PY'
import json
from pathlib import Path
before = json.loads(Path('rocket.perveance.json').read_text())
after = json.loads(Path('rocket.perveance.v001.json').read_text())
report = json.loads(Path('inspected.json').read_text())
assert report['ok'] and len(report['layers']) == len(before['layers']) == 4
assert before['layers'][1:] == after['layers'][1:]
assert sum(a != b for a, b in zip(before['layers'][0]['pixels'], after['layers'][0]['pixels'])) == 1
assert report['revision'] != json.loads(Path('created.json').read_text())['revision']
proposal = {'version': 1, 'base_revision': report['revision'],
            'operations': [{'op': 'pixel', 'layer': 0, 'x': 31, 'y': 31, 'color': [88,77,66,55]}]}
Path('proposal.json').write_text(json.dumps(proposal))
PY
python3 "$bridge" apply-proposal rocket.perveance.v001.json final.json proposal.json > applied.json
python3 "$bridge" export-png final.json bridge.png
"$cli" export-png final.json rust.png
"$cli" sample final.json 31 31 layer 0 > sample.json
python3 - <<'PY'
import json
from PIL import Image
assert json.load(open('sample.json'))['color'] == [88,77,66,55]
with Image.open('bridge.png') as a, Image.open('rust.png') as b:
    assert a.size == b.size == (32, 32)
    assert a.convert('RGBA').tobytes() == b.convert('RGBA').tobytes()
PY
echo 'Agent project, GUI edit, revision-bound agent edit and PNG parity passed.'
