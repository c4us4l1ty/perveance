import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / "tools" / "arena_art.py"
spec = importlib.util.spec_from_file_location("arena_art", SCRIPT)
art = importlib.util.module_from_spec(spec)
spec.loader.exec_module(art)


class ArenaArtTests(unittest.TestCase):
    def test_composite_and_hidden_raw_pixels(self):
        p = art.create_project(1, 1, [
            {"op": "pixel", "layer": 0, "x": 0, "y": 0, "color": [255, 0, 0, 255]},
            {"op": "add_layer"},
            {"op": "pixel", "layer": 1, "x": 0, "y": 0, "color": [0, 0, 255, 128]},
            {"op": "add_layer"},
            {"op": "pixel", "layer": 2, "x": 0, "y": 0, "color": [7, 9, 11, 0]},
            {"op": "set_visibility", "layer": 2, "visible": False},
        ])
        self.assertEqual(list(art.rgba_composite(p)), [127, 0, 128, 255])
        self.assertEqual(p["layers"][2]["pixels"][0], [7, 9, 11, 0])
        self.assertTrue(art.png_bytes(p).startswith(b"\x89PNG\r\n\x1a\n"))

    def test_invalid_inputs_are_rejected_before_writing(self):
        bad_ops = [
            [{"op": "pixel", "layer": 0, "x": True, "y": 0, "color": [1, 2, 3, 4]}],
            [{"op": "pixel", "layer": 0, "x": 0, "y": 0, "color": [1, 2, 3, 4], "extra": 1}],
            [{"op": "rectangle", "layer": 0, "x": 1, "y": 1, "width": 2, "height": 1, "color": [0, 0, 0, 0]}],
            [{"op": "execute", "command": "anything"}],
        ]
        for ops in bad_ops:
            with self.assertRaises(ValueError):
                art.create_project(2, 2, ops)
        with self.assertRaises(ValueError):
            art.create_project(129, 1, [])
        with self.assertRaises(ValueError):
            art.create_project(1, 1, [{"op": "add_layer"}] * 16)
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "ops.json"
            path.write_text('[{"op":"add_layer","op":"add_layer"}]')
            with self.assertRaises(ValueError):
                art.load_json(path, 1000)
            path.write_bytes(b" " * (art.MAX_BATCH_BYTES + 1))
            with self.assertRaises(ValueError):
                art.load_json(path, art.MAX_BATCH_BYTES)
            output = Path(tmp) / "out.json"
            result = subprocess.run([sys.executable, str(SCRIPT), "create", "2", "2", str(path), str(output)], capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse(output.exists())

    def test_apply_is_atomic_and_preserves_input_snapshot(self):
        p = art.create_project(2, 2, [])
        before = json.dumps(p)
        with self.assertRaises(ValueError):
            art.apply_operations(p, [
                {"op": "pixel", "layer": 0, "x": 0, "y": 0, "color": [1, 2, 3, 4]},
                {"op": "pixel", "layer": 9, "x": 0, "y": 0, "color": [1, 2, 3, 4]},
            ])
        self.assertEqual(json.dumps(p), before)

    def test_revision_proposals_reject_stale_and_unknown_fields(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            project = root / "project.json"
            proposal = root / "proposal.json"
            output = root / "edited.json"
            source = art.create_project(2, 2, [])
            # Hashing is independent of JSON input key order.
            reordered = dict(reversed(list(source.items())))
            self.assertEqual(art.project_revision(source), art.project_revision(reordered))
            project.write_text(json.dumps(reordered))
            payload = {"version": 1, "base_revision": art.project_revision(source), "operations": [
                {"op": "pixel", "layer": 0, "x": 1, "y": 1, "color": [1, 2, 3, 4]}]}
            proposal.write_text(json.dumps(payload))
            response = art.main(["apply-proposal", str(project), str(output), str(proposal)])
            self.assertEqual(response["base_revision"], payload["base_revision"])
            self.assertEqual(art.load_json(output, art.MAX_PROJECT_BYTES)["layers"][0]["pixels"][3], [1, 2, 3, 4])
            bad = root / "bad.json"
            payload["base_revision"] = "stale"
            proposal.write_text(json.dumps(payload))
            with self.assertRaises(ValueError):
                art.main(["apply-proposal", str(project), str(bad), str(proposal)])
            self.assertFalse(bad.exists())
            payload["permit"] = True
            proposal.write_text(json.dumps(payload))
            with self.assertRaises(ValueError):
                art.main(["apply-proposal", str(project), str(bad), str(proposal)])
            self.assertFalse(bad.exists())

    def test_no_clobber_and_local_agent_flow(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            ops = root / "ops.json"
            project = root / "art.json"
            png = root / "art.png"
            ops.write_text(json.dumps([{"op": "rectangle", "layer": 0, "x": 1, "y": 1, "width": 2, "height": 2, "color": [12, 34, 56, 255]}]))
            commands = [["create", "4", "4", str(ops), str(project)], ["export-png", str(project), str(png)]]
            for command in commands:
                result = subprocess.run([sys.executable, str(SCRIPT), *command], capture_output=True)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertTrue(json.loads(result.stdout)["ok"])
            self.assertEqual(json.loads(project.read_text())["layers"][0]["pixels"][5], [12, 34, 56, 255])
            sampled = subprocess.run([sys.executable, str(SCRIPT), "sample", str(project), "1", "1", "composite"], capture_output=True, check=True)
            self.assertEqual(json.loads(sampled.stdout)["color"], [12, 34, 56, 255])
            inspected = subprocess.run([sys.executable, str(SCRIPT), "inspect", str(project)], capture_output=True, check=True)
            self.assertEqual(json.loads(inspected.stdout)["width"], 4)
            before = project.read_bytes()
            result = subprocess.run([sys.executable, str(SCRIPT), *commands[0]], capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(project.read_bytes(), before)
            self.assertEqual(sorted(p.name for p in root.iterdir()), ["art.json", "art.png", "ops.json"])


if __name__ == "__main__":
    unittest.main()
