import hashlib
from pathlib import Path
import subprocess
import tarfile
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "tools" / "package_linux.sh"


class LinuxPackageTests(unittest.TestCase):
    def test_package_contents_checksums_permissions_and_no_clobber(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            binaries = root / "binaries"
            binaries.mkdir()
            for name in ("perveance", "perveance-cli"):
                path = binaries / name
                path.write_text("#!/bin/sh\nexit 0\n")
                path.chmod(0o755)
            output = root / "bundle"
            result = subprocess.run([SCRIPT, binaries, output], capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            archive = output / "perveance-linux-x86_64.tar.gz"
            digest = hashlib.sha256(archive.read_bytes()).hexdigest()
            self.assertEqual((output / "SHA256SUMS").read_text(), digest + "  " + archive.name + "\n")
            with tarfile.open(archive, "r:gz") as tar:
                prefix = "perveance-linux-x86_64/"
                for name in ("bin/perveance", "bin/perveance-cli"):
                    self.assertEqual(tar.getmember(prefix + name).mode & 0o777, 0o755)
                for name in ("README.txt", "BUILD-INFO.txt", "examples/potion-operations.json", "examples/rocket-operations.json"):
                    self.assertTrue(tar.getmember(prefix + name).isfile())
                info = tar.extractfile(prefix + "BUILD-INFO.txt").read().decode()
                self.assertIn("Source revision:", info)
                self.assertIn("Working tree dirty:", info)
            before = archive.read_bytes()
            retry = subprocess.run([SCRIPT, binaries, output], capture_output=True)
            self.assertNotEqual(retry.returncode, 0)
            self.assertEqual(archive.read_bytes(), before)

    def test_missing_binary_does_not_create_output(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            output = root / "bundle"
            result = subprocess.run([SCRIPT, root, output], capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse(output.exists())


if __name__ == "__main__":
    unittest.main()
