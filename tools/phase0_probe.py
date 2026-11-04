#!/usr/bin/env python3
"""Non-destructive development-environment probe; this is NOT a security certification."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
from typing import Any


def run_command(command: list[str], timeout: int = 8) -> dict[str, Any]:
    try:
        result = subprocess.run(command, capture_output=True, text=True, timeout=timeout, check=False)
    except (OSError, subprocess.TimeoutExpired) as exc:
        return {"ok": False, "detail": type(exc).__name__}
    return {
        "ok": result.returncode == 0,
        "exit_code": result.returncode,
        "stdout": result.stdout.strip()[:1000],
        "stderr": result.stderr.strip()[:1000],
    }


def rust_toolchain() -> dict[str, Any]:
    tools = {}
    for name in ("rustc", "cargo"):
        path = shutil.which(name)
        tools[name] = {"available": path is not None}
        if path:
            version = run_command([path, "--version"])
            tools[name]["version"] = version.get("stdout", "") if version["ok"] else "unavailable"
    return tools


def namespace_probe() -> dict[str, Any]:
    """Check namespace availability and visibility only; no containment claim is made."""
    unshare = shutil.which("unshare")
    if not unshare or sys.platform != "linux":
        return {"available": False, "detail": "Linux unshare is unavailable"}
    # The child only reads a sentinel created for this test, not any private host files.
    with tempfile.TemporaryDirectory(prefix="perveance-probe-") as tmp:
        sentinel = Path(tmp) / "sentinel"
        sentinel.write_text("perveance-probe", encoding="utf-8")
        code = (
            "import json, pathlib, sys; "
            "p=pathlib.Path(sys.argv[1]); "
            "print(json.dumps({'sentinel_visible': p.is_file() and "
            "p.read_text(encoding='utf-8') == 'perveance-probe', "
            "'network_interfaces': sorted(line.split(':', 1)[0].strip() for line in "
            "pathlib.Path('/proc/net/dev').read_text(encoding='utf-8').splitlines()[2:] if ':' in line)}))"
        )
        result = run_command([unshare, "-Urn", sys.executable, "-c", code, str(sentinel)])
    if not result["ok"]:
        return {"available": False, "detail": result.get("stderr") or result.get("detail", "unshare failed")}
    try:
        observations = json.loads(result["stdout"])
    except (json.JSONDecodeError, KeyError):
        return {"available": False, "detail": "unexpected namespace probe response"}
    return {"available": True, **observations}


def build_report() -> dict[str, Any]:
    return {
        "platform": sys.platform,
        "rust": rust_toolchain(),
        "isolation_tools": {name: bool(shutil.which(name)) for name in ("unshare", "bwrap", "podman", "docker")},
        "namespace_probe": namespace_probe(),
        "autonomous_execution": "disabled: no verified supervisor or sandbox is implemented",
        "warning": "A successful namespace probe does not establish a security boundary.",
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--json", action="store_true", help="print structured JSON")
    args = parser.parse_args()
    report = build_report()
    if args.json:
        print(json.dumps(report, indent=2, sort_keys=True))
    else:
        print("Perveance feasibility probe — diagnostic only")
        print(f"Platform: {report['platform']}")
        for name, value in report["rust"].items():
            print(f"{name}: {value.get('version', 'not found')}")
        print("Isolation tools: " + ", ".join(f"{k}={'found' if v else 'not found'}" for k, v in report["isolation_tools"].items()))
        ns = report["namespace_probe"]
        if ns["available"]:
            print(f"User + network namespace: available; host sentinel visible: {ns['sentinel_visible']}; interfaces: {', '.join(ns['network_interfaces'])}")
        else:
            print(f"User + network namespace: unavailable ({ns['detail']})")
        print(report["warning"])
        print(f"Autonomous execution: {report['autonomous_execution']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
