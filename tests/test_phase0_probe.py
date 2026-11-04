import importlib.util
from pathlib import Path
import subprocess
import sys
import unittest
from unittest.mock import patch


SPEC = importlib.util.spec_from_file_location("phase0_probe", Path(__file__).resolve().parents[1] / "tools" / "phase0_probe.py")
probe = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(probe)


class ProbeTests(unittest.TestCase):
    def test_missing_tools_are_reported(self):
        with patch.object(probe.shutil, "which", return_value=None):
            result = probe.rust_toolchain()
        self.assertFalse(result["cargo"]["available"])
        self.assertFalse(result["rustc"]["available"])

    def test_missing_unshare_is_not_success(self):
        with patch.object(probe.shutil, "which", return_value=None):
            result = probe.namespace_probe()
        self.assertFalse(result["available"])

    def test_command_timeout_is_reported(self):
        with patch.object(probe.subprocess, "run", side_effect=subprocess.TimeoutExpired(["x"], 1)):
            result = probe.run_command(["x"])
        self.assertEqual(result, {"ok": False, "detail": "TimeoutExpired"})

    def test_probe_json_and_disabled_state(self):
        cmd = [sys.executable, str(Path(__file__).resolve().parents[1] / "tools" / "phase0_probe.py"), "--json"]
        output = subprocess.run(cmd, check=True, capture_output=True, text=True, timeout=15)
        import json
        report = json.loads(output.stdout)
        self.assertIn("disabled", report["autonomous_execution"])
        self.assertIn("does not establish", report["warning"])


if __name__ == "__main__":
    unittest.main()
