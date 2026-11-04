import importlib.util
import json
import os
import shutil
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
        workspaces = []
        def fake_popen(command, **kwargs):
            self.assertEqual(command[0], "/test/bwrap")
            for option in ("--unshare-user", "--unshare-pid", "--unshare-net", "--unshare-ipc", "--unshare-uts", "--die-with-parent", "--clearenv", "--tmpfs"):
                self.assertIn(option, command)
            self.assertEqual(command[command.index("-c") + 1], probe.WORKER)
            self.assertIn("--bind", command)
            self.assertEqual(command[command.index("--bind") + 2], "/work")
            self.assertEqual(kwargs["env"]["PERVEANCE_PROBE_SECRET"], "probe-only-environment-sentinel")
            self.assertNotIn("HOME", kwargs["env"])
            checks = {name: True for name in probe.CHECKS}
            workspace = Path(command[command.index("--bind") + 1])
            workspaces.append(workspace.name)
            self.assertEqual((workspace / "input.fixture").read_bytes(), b"fixed-probe-input")
            self.assertEqual((workspace / "host.sock").exists(), workspace.name == "unsafe-workspace")
            if workspace.name == "unsafe-workspace":
                checks["workspace_host_socket_unreachable"] = False
            kwargs["stdout"].write(json.dumps(checks).encode())
            process = Mock()
            process.wait.return_value = 0
            return process
        with patch.object(probe.subprocess, "Popen", side_effect=fake_popen):
            result = probe.run_fixed_probe("/test/bwrap")
        self.assertEqual(result["status"], "passed")
        self.assertEqual(workspaces, ["clean-workspace", "unsafe-workspace"])
        self.assertFalse(result["unsafe_workspace_control"]["workspace_host_socket_unreachable"])
        self.assertEqual(result["autonomous_execution"], "disabled")

    def test_unsafe_control_is_required_and_strict(self):
        def fake_popen(command, **kwargs):
            # A missing or falsely safe control must never become a pass.
            checks = {name: True for name in probe.CHECKS}
            kwargs["stdout"].write(json.dumps(checks).encode())
            process = Mock()
            process.wait.return_value = 0
            return process
        with patch.object(probe.subprocess, "Popen", side_effect=fake_popen):
            result = probe.run_fixed_probe("/test/bwrap")
        self.assertEqual(result["status"], "failed")
        self.assertEqual(result["autonomous_execution"], "disabled")
        self.assertTrue(result["unsafe_workspace_control"]["workspace_host_socket_unreachable"])

    def test_fixed_worker_detects_unisolated_access(self):
        # This deliberately runs only the fixed probe code without bubblewrap, against
        # disposable fixtures. It proves the checks detect an unconfined environment.
        with tempfile.TemporaryDirectory() as tmp, socket.socket(socket.AF_INET, socket.SOCK_STREAM) as listener, socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as unix_listener:
            root = Path(tmp)
            sentinel = root / "host-sentinel"
            sentinel.write_bytes(b"probe-only-secret")
            workspace = root / "workspace"
            workspace.mkdir()
            (workspace / "outside-link").symlink_to(sentinel)
            (workspace / "input.fixture").write_bytes(b"fixed-probe-input")
            unix_listener.bind(str(workspace / "host.sock"))
            unix_listener.listen(1)
            listener.bind(("127.0.0.1", 0))
            listener.listen(1)
            result = subprocess.run(
                [sys.executable, "-I", "-c", probe.WORKER, str(sentinel), str(listener.getsockname()[1]), str(workspace), str(workspace / "host.sock")],
                env={"PATH": "/usr/bin:/bin", "PERVEANCE_PROBE_SECRET": "probe-only-environment-sentinel"},
                capture_output=True, check=True, timeout=10,
            )
            checks = json.loads(result.stdout)
            self.assertEqual(probe.classify(checks), "failed")
            self.assertTrue(checks["workspace_read_write"])
            self.assertTrue(checks["staged_input_readable"])
            for name in set(probe.CHECKS) - {"workspace_read_write", "staged_input_readable"}:
                self.assertFalse(checks[name], name)
            self.assertEqual(sentinel.read_bytes(), b"changed-by-symlink")

    @unittest.skipUnless(sys.platform == "linux" and shutil.which("unshare"), "Linux unshare is required")
    def test_network_namespace_does_not_isolate_exposed_host_unix_socket(self):
        # Fixed disposable worker only. This deliberately shows why a shared
        # workspace must not carry a host socket into a future worker.
        with tempfile.TemporaryDirectory() as tmp, socket.socket(socket.AF_INET, socket.SOCK_STREAM) as listener, socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as unix_listener:
            root = Path(tmp)
            sentinel = root / "host-sentinel"
            sentinel.write_bytes(b"probe-only-secret")
            workspace = root / "workspace"
            workspace.mkdir()
            (workspace / "outside-link").symlink_to(sentinel)
            (workspace / "input.fixture").write_bytes(b"fixed-probe-input")
            unix_listener.bind(str(workspace / "host.sock"))
            unix_listener.listen(2)
            with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as control:
                control.connect(str(workspace / "host.sock"))
                accepted, _ = unix_listener.accept()
                accepted.close()
            listener.bind(("127.0.0.1", 0))
            listener.listen(1)
            result = subprocess.run(
                [shutil.which("unshare"), "-Urn", sys.executable, "-I", "-c", probe.WORKER, str(sentinel), str(listener.getsockname()[1]), str(workspace), str(workspace / "host.sock")],
                env={"PATH": "/usr/bin:/bin", "PERVEANCE_PROBE_SECRET": "probe-only-environment-sentinel"},
                capture_output=True, timeout=10,
            )
            if result.returncode != 0 and any(reason in result.stderr for reason in (b"Operation not permitted", b"Permission denied", b"Invalid argument", b"Function not implemented")):
                self.skipTest("unprivileged user/network namespace unavailable")
            self.assertEqual(result.returncode, 0, result.stderr.decode(errors="replace"))
            checks = json.loads(result.stdout)
            self.assertTrue(checks["host_loopback_blocked"])
            self.assertFalse(checks["workspace_host_socket_unreachable"])
            self.assertEqual(probe.classify(checks), "failed")
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
