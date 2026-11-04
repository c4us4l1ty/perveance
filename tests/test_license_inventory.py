import copy
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / "tools" / "license_inventory.py"
spec = importlib.util.spec_from_file_location("license_inventory", SCRIPT)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class LicenseInventoryTests(unittest.TestCase):
    def fixture(self):
        return {
            "workspace_members": ["app"],
            "resolve": {"nodes": [{"id": "app"}, {"id": "a"}, {"id": "b"}, {"id": "c"}]},
            "packages": [
                {"id": "app", "name": "app", "version": "1", "source": None, "license": None},
                {"id": "a", "name": "alpha", "version": "2", "source": "registry+https://secret.example", "license": "MIT OR Apache-2.0", "license_file": "/private/path"},
                {"id": "b", "name": "beta", "version": "3", "source": "git+https://secret.example", "license": None, "license_file": "LICENSE"},
                {"id": "c", "name": "gamma", "version": "4", "source": None, "license": "Custom"},
                {"id": "unresolved", "name": "omitted", "version": "1", "source": None},
            ],
        }

    def test_inventory_is_resolved_sorted_and_does_not_disclose_paths_or_urls(self):
        data = module.inventory(self.fixture())
        self.assertEqual(data["dependency_count"], 3)
        self.assertEqual(data["without_license_expression"], 1)
        self.assertEqual([p["name"] for p in data["dependencies"]], ["alpha", "beta", "gamma"])
        self.assertEqual([p["source_kind"] for p in data["dependencies"]], ["registry", "git", "path"])
        self.assertTrue(data["dependencies"][0]["has_license_file_metadata"])
        self.assertTrue(data["dependencies"][1]["has_license_file_metadata"])
        self.assertFalse(data["dependencies"][2]["has_license_file_metadata"])
        self.assertNotIn("private/path", str(data))
        self.assertNotIn("secret.example", str(data))

    def test_command_outputs_json_and_rejects_invalid_metadata(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "metadata.json"
            path.write_text(json.dumps(self.fixture()), encoding="utf-8")
            result = subprocess.run([sys.executable, str(SCRIPT), str(path)], capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(json.loads(result.stdout)["dependency_count"], 3)
            path.write_text("{}", encoding="utf-8")
            result = subprocess.run([sys.executable, str(SCRIPT), str(path)], capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(result.stdout, "")

    def test_malformed_or_inconsistent_metadata_fails(self):
        data = self.fixture()
        data["resolve"]["nodes"].append({"id": "missing"})
        with self.assertRaises(ValueError):
            module.inventory(data)
        data = self.fixture()
        data["packages"].append(copy.deepcopy(data["packages"][1]))
        with self.assertRaises(ValueError):
            module.inventory(data)
        data = self.fixture()
        data["packages"][1]["license"] = ["MIT"]
        with self.assertRaises(ValueError):
            module.inventory(data)


if __name__ == "__main__":
    unittest.main()
