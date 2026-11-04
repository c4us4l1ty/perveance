import importlib.util
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import sys
import unittest
from unittest.mock import Mock, patch

SPEC = importlib.util.spec_from_file_location("isolation_probe", Path(__file__).resolve().parents[1] / "tools" / "isolation_probe.py")
probe = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(probe)


class IsolationProbeTests(unittest.TestCase):
    def test_success_requires_all_exact_boolean_checks(self):
        all_pass = {name: True for name in probe.CHECKS}
        self.assertEqual(probe.classify(all_pass), "passed")
        for name in probe.CHECKS:
            checks = dict(all_pass)
            checks[name] = False
            self.assertEqual(probe.classify(checks), "failed")
        self.assertEqual(probe.classify({}), "failed")
        self.assertEqual(probe.classify({**all_pass, "extra": True}), "failed")
        self.assertEqual(probe.classify({**all_pass, probe.CHECKS[0]: 1}), "failed")
        self.assertEqual(probe.classify([]), "failed")

    def test_missing_bwrap_is_unavailable_and_never_enables_execution(self):
        with patch.object(probe.sys, "platform", "linux"), patch.object(probe.shutil, "which", return_value=None):
            result = probe.build_report()
        self.assertEqual(result["status"], "unavailable")
        self.assertEqual(result["autonomous_execution"], "disabled")

    def test_unsupported_platform_does_not_invoke_bwrap(self):
        with patch.object(probe.sys, "platform", "win32"), patch.object(probe.shutil, "which") as which:
            result = probe.build_report()
        which.assert_not_called()
        self.assertEqual(result["status"], "unavailable")
        self.assertEqual(result["autonomous_execution"], "disabled")

    def test_launch_uses_fixed_code_and_required_namespaces(self):
        def fake_popen(command, **kwargs):
            self.assertEqual(command[0], "/test/bwrap")
            for option in ("--unshare-user", "--unshare-pid", "--unshare-net", "--unshare-ipc", "--unshare-uts", "--die-with-parent", "--clearenv", "--tmpfs"):
                self.assertIn(option, command)
            self.assertEqual(command[command.index("-c") + 1], probe.WORKER)
            self.assertEqual(kwargs["env"]["PERVEANCE_PROBE_SECRET"], "probe-only-environment-sentinel")
            self.assertNotIn("HOME", kwargs["env"])
            kwargs["stdout"].write(json.dumps({name: True for name in probe.CHECKS}).encode())
            process = Mock()
            process.wait.return_value = 0
            return process
        with patch.object(probe.subprocess, "Popen", side_effect=fake_popen):
            result = probe.run_fixed_probe("/test/bwrap")
        self.assertEqual(result["status"], "passed")
        self.assertEqual(result["autonomous_execution"], "disabled")

    def test_fixed_worker_detects_unisolated_access(self):
        # This deliberately runs only the fixed probe code without bubblewrap, against
        # disposable fixtures. It proves the checks detect an unconfined environment.
        with tempfile.TemporaryDirectory() as tmp, socket.socket(socket.AF_INET, socket.SOCK_STREAM) as listener:
            root = Path(tmp)
            sentinel = root / "host-sentinel"
            sentinel.write_bytes(b"probe-only-secret")
            workspace = root / "workspace"
            workspace.mkdir()
            (workspace / "outside-link").symlink_to(sentinel)
            listener.bind(("127.0.0.1", 0))
            listener.listen(1)
            result = subprocess.run(
                [sys.executable, "-I", "-c", probe.WORKER, str(sentinel), str(listener.getsockname()[1]), str(workspace)],
                env={"PATH": "/usr/bin:/bin", "PERVEANCE_PROBE_SECRET": "probe-only-environment-sentinel"},
                capture_output=True, check=True, timeout=10,
            )
            checks = json.loads(result.stdout)
            self.assertEqual(probe.classify(checks), "failed")
            self.assertTrue(checks["workspace_read_write"])
            for name in set(probe.CHECKS) - {"workspace_read_write"}:
                self.assertFalse(checks[name], name)
            self.assertEqual(sentinel.read_bytes(), b"changed-by-symlink")

    def test_tool_failure_and_timeout_fail_closed(self):
        process = Mock()
        process.wait.return_value = 1
        with patch.object(probe.subprocess, "Popen", return_value=process):
            result = probe.run_fixed_probe("/test/bwrap")
        self.assertEqual(result["status"], "failed")
        self.assertEqual(result["autonomous_execution"], "disabled")
        process = Mock()
        process.pid = 987
        process.wait.side_effect = [subprocess.TimeoutExpired("bwrap", 10), 0]
        with patch.object(probe.subprocess, "Popen", return_value=process), patch.object(probe.os, "killpg") as killpg:
            result = probe.run_fixed_probe("/test/bwrap")
        killpg.assert_called_once_with(987, probe.signal.SIGKILL)
        self.assertEqual(result["status"], "failed")
        self.assertEqual(result["autonomous_execution"], "disabled")


if __name__ == "__main__":
    unittest.main()
