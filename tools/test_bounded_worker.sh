#!/usr/bin/env bash
# Run ONLY inside the disposable delegated systemd CI service. Never changes
# global user-namespace/AppArmor policy or grants application code sudo rights.
set -euo pipefail
if [[ $# != 1 || $(id -u) == 0 ]]; then
    echo 'Run as an unprivileged user inside a delegated CI service, with one compiled test executable.' >&2
    exit 1
fi
root=$(python3 - <<'PY'
from pathlib import Path
lines = Path('/proc/self/cgroup').read_text().splitlines()
assert len(lines) == 1 and lines[0].startswith('0::/'), 'unified cgroup v2 required'
relative = Path(lines[0][3:])
assert relative.name == 'supervisor' and '..' not in relative.parts, 'DelegateSubgroup=supervisor required'
root = Path('/sys/fs/cgroup') / relative.relative_to('/').parent
assert not (root / 'cgroup.procs').read_text().strip(), 'delegated root must be empty'
print(root)
PY
)
chmod 700 "$root"
printf '+cpu +memory +pids' > "$root/cgroup.subtree_control"
export PERVEANCE_TEST_CGROUP_ROOT="$root"
"$1" --ignored --test-threads=1 --nocapture
python3 - "$root" <<'PY'
from pathlib import Path
import sys
root = Path(sys.argv[1])
assert not any(p.name.startswith('perveance-worker-') for p in root.iterdir()), 'worker cgroup leaked'
print('Delegated cgroup diagnostics finished without managed cgroup leaks; autonomy remains disabled.')
PY
