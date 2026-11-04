#!/usr/bin/env python3
"""Run a fixed, non-destructive bubblewrap feasibility probe. Not a sandbox certification.

This program never accepts or runs user/model code. A pass does not enable Perveance
agent execution; it observes only the small set of behaviors named in the report.
"""
from __future__ import annotations

import json
import os
from pathlib import Path
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
from typing import Any

CHECKS = (
    "workspace_read_write", "staged_input_readable", "host_secret_hidden", "host_write_blocked",
    "symlink_escape_blocked", "proc_root_escape_blocked", "host_loopback_blocked", "host_socket_path_hidden", "workspace_host_socket_unreachable", "environment_cleared",
)
LIMITATIONS = [
    "Fixed test code only; a pass is not a sandbox certification or an execution authorization.",
    "No kernel escape, syscall, GPU, cgroup/quota, network egress, dependency, credential-broker, or persistence audit.",
    "The diagnostic binds host /usr, /bin and libraries read-only; it is not a production worker profile.",
    "Only host IPv4 loopback and a planted workspace Unix socket are tested, not all network protocols or destinations.",
    "The positive fixture contains no host socket; this checks fixture staging, not an OS socket filter.",
    "The negative control deliberately exposes a host Unix socket, which may remain reachable across network namespaces.",
    "Probe output and tool availability are not trusted authorization for autonomous execution.",
]

# This fixed program inspects only a probe-created sentinel and local listener.
# Its arguments, paths and listener are created by this diagnostic itself.
WORKER = r'''
import json, os, pathlib, socket, sys
secret = pathlib.Path(sys.argv[1])
port = int(sys.argv[2])
work = pathlib.Path(sys.argv[3])
host_socket = pathlib.Path(sys.argv[4])
checks = {}
try:
    checks['staged_input_readable'] = (work / 'input.fixture').read_bytes() == b'fixed-probe-input'
except OSError:
    checks['staged_input_readable'] = False
p = work / 'scratch.txt'
try:
    p.write_text('probe-workspace', encoding='utf-8')
    checks['workspace_read_write'] = p.read_text(encoding='utf-8') == 'probe-workspace'
except OSError:
    checks['workspace_read_write'] = False
try:
    secret.read_bytes()
    checks['host_secret_hidden'] = False
except OSError:
    checks['host_secret_hidden'] = True
try:
    secret.write_bytes(b'changed-by-probe')
    checks['host_write_blocked'] = False
except OSError:
    checks['host_write_blocked'] = True
try:
    (work / 'outside-link').read_bytes()
    symlink_read_blocked = False
except OSError:
    symlink_read_blocked = True
try:
    (work / 'outside-link').write_bytes(b'changed-by-symlink')
    symlink_write_blocked = False
except OSError:
    symlink_write_blocked = True
checks['symlink_escape_blocked'] = symlink_read_blocked and symlink_write_blocked
checks['proc_root_escape_blocked'] = True
for prefix in ('/proc/1/root', '/proc/self/root'):
    try:
        pathlib.Path(prefix + str(secret)).read_bytes()
        checks['proc_root_escape_blocked'] = False
    except OSError:
        pass
try:
    with socket.create_connection(('127.0.0.1', port), timeout=1):
        checks['host_loopback_blocked'] = False
except OSError:
    checks['host_loopback_blocked'] = True
try:
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
        client.settimeout(1)
        client.connect(str(host_socket))
        checks['host_socket_path_hidden'] = False
except OSError:
    checks['host_socket_path_hidden'] = True
try:
    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as client:
        client.settimeout(1)
        client.connect(str(work / 'host.sock'))
        checks['workspace_host_socket_unreachable'] = False
except OSError:
    checks['workspace_host_socket_unreachable'] = True
checks['environment_cleared'] = 'PERVEANCE_PROBE_SECRET' not in os.environ
print(json.dumps(checks, sort_keys=True))
'''


def classify(checks: Any) -> str:
    """Only the complete, correctly typed expected response can pass."""
    if not isinstance(checks, dict) or set(checks) != set(CHECKS):
        return "failed"
    return "passed" if all(checks[name] is True for name in CHECKS) else "failed"


def report(status: str, reason: str, checks: dict[str, Any] | None = None, unsafe_control: dict[str, Any] | None = None) -> dict[str, Any]:
    return {
        "probe": "bubblewrap-fixed-checks-v3",
        "status": status,
        "reason": reason,
        "checks": checks or {},
        "unsafe_workspace_control": unsafe_control or {},
        "autonomous_execution": "disabled",
        "limitations": LIMITATIONS,
    }


def run_fixed_probe(bwrap: str) -> dict[str, Any]:
    with tempfile.TemporaryDirectory(prefix="perveance-isolation-probe-") as tmp:
        root = Path(tmp)
        secret = root / "host-sentinel"
        secret.write_bytes(b"probe-only-secret")
        clean = root / "clean-workspace"
        unsafe = root / "unsafe-workspace"
        clean.mkdir()
        unsafe.mkdir()
        # Both workspaces include a hostile symlink; only the unsafe control
        # deliberately mounts a live host socket. No input directory is copied.
        for workspace in (clean, unsafe):
            (workspace / "outside-link").symlink_to(secret)
            (workspace / "input.fixture").write_bytes(b"fixed-probe-input")
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as listener, socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as unix_listener:
            socket_path = unsafe / "host.sock"
            unix_listener.bind(str(socket_path))
            unix_listener.listen(4)
            unix_listener.settimeout(2)
            with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as control:
                control.settimeout(2)
                control.connect(str(socket_path))
                accepted, _ = unix_listener.accept()
                accepted.close()
            listener.bind(("127.0.0.1", 0))
            listener.listen(4)
            listener.settimeout(2)
            port = listener.getsockname()[1]
            with socket.create_connection(("127.0.0.1", port), timeout=2):
                accepted, _ = listener.accept()
                accepted.close()

            def run_worker(workspace: Path) -> tuple[dict[str, Any] | None, str | None]:
                command = [bwrap, "--unshare-user", "--unshare-pid", "--unshare-net",
                           "--unshare-ipc", "--unshare-uts", "--new-session", "--die-with-parent",
                           "--clearenv", "--setenv", "PATH", "/usr/bin:/bin"]
                for path in ("/usr", "/bin", "/lib", "/lib64"):
                    if Path(path).exists():
                        command.extend(["--ro-bind", path, path])
                command.extend(["--proc", "/proc", "--dev", "/dev", "--tmpfs", "/tmp",
                                "--bind", str(workspace), "/work", "--chdir", "/work",
                                "/usr/bin/python3", "-I", "-c", WORKER, str(secret), str(port), "/work", str(socket_path)])
                # Fixed code only; keep output bounded even on failures.
                import resource
                def limit_output() -> None:
                    resource.setrlimit(resource.RLIMIT_FSIZE, (65536, 65536))
                with tempfile.TemporaryFile() as stdout, tempfile.TemporaryFile() as stderr:
                    try:
                        process = subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=stdout, stderr=stderr,
                                                   env={"PATH": "/usr/bin:/bin", "PERVEANCE_PROBE_SECRET": "probe-only-environment-sentinel"}, start_new_session=True,
                                                   preexec_fn=limit_output)
                        try:
                            exit_code = process.wait(timeout=10)
                        except subprocess.TimeoutExpired:
                            os.killpg(process.pid, signal.SIGKILL)
                            process.wait()
                            return None, "probe timed out; no execution permission granted"
                    except OSError as exc:
                        return None, f"could not start probe: {type(exc).__name__}"
                    stdout.seek(0)
                    stderr.seek(0)
                    output = stdout.read(65537)
                    error = stderr.read(2048).decode("utf-8", errors="replace").strip()
                if exit_code != 0:
                    return None, "bubblewrap or fixed test failed: " + (error[:500] or f"exit {exit_code}")
                if len(output) > 65536:
                    return None, "probe output exceeded limit"
                try:
                    result = json.loads(output)
                except (ValueError, UnicodeError):
                    return None, "unexpected probe output"
                if not isinstance(result, dict):
                    return None, "unexpected probe output"
                return result, None

            checks, error = run_worker(clean)
            if error:
                return report("failed", "clean workspace: " + error)
            if secret.read_bytes() != b"probe-only-secret":
                return report("failed", "host sentinel was unexpectedly modified", checks)
            if classify(checks) != "passed":
                return report("failed", "one or more clean-workspace checks failed", checks)
            control, error = run_worker(unsafe)
            if error:
                return report("failed", "unsafe workspace control: " + error, checks)
            if secret.read_bytes() != b"probe-only-secret":
                return report("failed", "host sentinel was unexpectedly modified", checks, control)
            # Validate the entire fixed response: only the planted socket check
            # should differ. The control proves the test detects this exposure.
            expected_control = {name: name != "workspace_host_socket_unreachable" for name in CHECKS}
            if set(control) != set(CHECKS) or any(control[name] is not expected for name, expected in expected_control.items()):
                return report("failed", "unsafe workspace control did not demonstrate host socket reachability", checks, control)
            return report("passed", "fixed clean-workspace checks passed; unsafe socket exposure detected", checks, control)


def build_report() -> dict[str, Any]:
    if sys.platform != "linux":
        return report("unavailable", "Linux is required")
    bwrap = shutil.which("bwrap")
    if not bwrap:
        return report("unavailable", "bubblewrap was not found")
    if not Path("/usr/bin/python3").is_file():
        return report("unavailable", "/usr/bin/python3 is unavailable")
    try:
        return run_fixed_probe(bwrap)
    except (OSError, ValueError) as exc:
        return report("failed", f"probe setup failed: {type(exc).__name__}")


def main() -> int:
    result = build_report()
    print(json.dumps(result, indent=2, sort_keys=True))
    # A pass means the fixed diagnostic ran as expected; it never authorizes execution.
    return 0 if result["status"] == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
