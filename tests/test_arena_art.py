import concurrent.futures
import importlib.util
import json
from pathlib import Path
import subprocess
import struct
import zlib
import sys
import tempfile
import threading
import unittest
from unittest import mock

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

    def test_scaled_png_is_nearest_neighbor_and_bounded(self):
        p = art.create_project(2, 1, [
            {"op": "pixel", "layer": 0, "x": 0, "y": 0, "color": [10, 20, 30, 255]},
            {"op": "pixel", "layer": 0, "x": 1, "y": 0, "color": [30, 20, 10, 128]},
        ])
        data = art.png_bytes(p, 3)
        self.assertEqual(struct.unpack(">II", data[16:24]), (6, 3))
        cursor = 8
        compressed = b""
        while cursor < len(data):
            length = struct.unpack(">I", data[cursor:cursor + 4])[0]
            kind = data[cursor + 4:cursor + 8]
            if kind == b"IDAT":
                compressed += data[cursor + 8:cursor + 8 + length]
            cursor += 12 + length
        row = b"\0" + bytes([10, 20, 30, 255]) * 3 + bytes([30, 20, 10, 128]) * 3
        self.assertEqual(zlib.decompress(compressed), row * 3)
        for factor in (0, 9, True):
            with self.assertRaises(ValueError):
                art.png_bytes(p, factor)

    def test_scaled_export_refuses_clobber(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            project = root / "project.json"
            output = root / "output.png"
            project.write_text(json.dumps(art.create_project(1, 1, [])))
            output.write_bytes(b"existing")
            with self.assertRaises(FileExistsError):
                art.main(["export-png-scaled", str(project), str(output), "2"])
            self.assertEqual(output.read_bytes(), b"existing")
            with self.assertRaises(ValueError):
                art.main(["export-png-scaled", str(project), str(root / "no.png"), "9"])
            self.assertFalse((root / "no.png").exists())

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

    def test_region_exposes_hidden_raw_pixels_and_checks_bounds(self):
        p = art.create_project(4, 3, [
            {"op": "add_layer"},
            {"op": "pixel", "layer": 1, "x": 2, "y": 1, "color": [7, 8, 9, 0]},
            {"op": "set_visibility", "layer": 1, "visible": False},
        ])
        raw = art.read_region(p, 1, 1, 2, 2, 1)
        visible = art.read_region(p, 1, 1, 2, 2)
        self.assertEqual(raw["pixels"][0][1], [7, 8, 9, 0])
        self.assertEqual(visible["pixels"][0][1], [0, 0, 0, 0])
        self.assertEqual(raw["revision"], visible["revision"])
        for x, y, width, height, layer in [(3, 0, 2, 1, None), (0, 0, 0, 1, None), (0, 0, 1, 1, 2), (True, 0, 1, 1, None)]:
            with self.assertRaises(ValueError):
                art.read_region(p, x, y, width, height, layer)
        big = art.create_project(128, 128, [])
        self.assertEqual(len(art.read_region(big, 0, 0, 64, 64)["pixels"]), 64)
        with self.assertRaises(ValueError):
            art.read_region(big, 0, 0, 65, 64)

    def test_region_results_do_not_alias_the_source_project(self):
        project = art.create_project(2, 1, [
            {"op": "pixel", "layer": 0, "x": 0, "y": 0, "color": [7, 8, 9, 0]},
            {"op": "set_visibility", "layer": 0, "visible": False},
        ])
        before = json.dumps(project)
        revision = art.project_revision(project)
        raw = art.read_region(project, 0, 0, 2, 1, 0)
        raw["pixels"][0][0][0] = 222
        raw["pixels"][0][1].append(999)
        raw["pixels"][0].append([1, 2, 3, 4])
        composite = art.read_region(project, 0, 0, 2, 1)
        composite["pixels"][0][0][0] = 333
        self.assertEqual(json.dumps(project), before)
        self.assertEqual(art.project_revision(project), revision)

    def test_no_clobber_installation_under_race_and_faults(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            output = root / "race.json"
            barrier = threading.Barrier(8, timeout=10)
            payloads = [bytes([i]) * 4096 for i in range(8)]
            def attempt(payload):
                barrier.wait()
                try:
                    art.write_new(output, payload)
                    return payload
                except FileExistsError:
                    return None
            with concurrent.futures.ThreadPoolExecutor(max_workers=8) as pool:
                winners = [result for result in pool.map(attempt, payloads) if result is not None]
            self.assertEqual(len(winners), 1)
            self.assertEqual(output.read_bytes(), winners[0])
            self.assertEqual(sorted(p.name for p in root.iterdir()), ["race.json"])
            for function in ("fsync", "link"):
                target = root / (function + ".json")
                with mock.patch.object(art.os, function, side_effect=OSError("injected failure")):
                    with self.assertRaisesRegex(OSError, "injected failure"):
                        art.write_new(target, b"data")
                self.assertFalse(target.exists())
                self.assertEqual(sorted(p.name for p in root.iterdir()), ["race.json"])
            if hasattr(art.os, "symlink"):
                link = root / "dangling.json"
                link.symlink_to(root / "missing.json")
                with self.assertRaises(FileExistsError):
                    art.write_new(link, b"data")
                self.assertTrue(link.is_symlink())
                self.assertFalse((root / "missing.json").exists())
                self.assertEqual(sorted(p.name for p in root.iterdir()), ["dangling.json", "race.json"])

    def test_layer_operations_and_resize_preserve_raw_pixels(self):
        p = art.create_project(3, 2, [
            {"op": "pixel", "layer": 0, "x": 2, "y": 1, "color": [5, 6, 7, 0]},
            {"op": "rename_layer", "layer": 0, "name": "雪" * 84},
            {"op": "set_visibility", "layer": 0, "visible": False},
            {"op": "duplicate_layer", "layer": 0},
            {"op": "flip_layer", "layer": 1, "axis": "horizontal"},
            {"op": "replace_color", "layer": 1, "from": [5, 6, 7, 0], "to": [8, 9, 10, 0]},
            {"op": "translate_layer", "layer": 1, "dx": 1, "dy": -1, "clip": False},
            {"op": "resize_canvas", "width": 4, "height": 3, "clip": False},
            {"op": "move_layer", "from": 1, "to": 0},
        ])
        self.assertEqual((p["width"], p["height"]), (4, 3))
        self.assertEqual(p["layers"][0]["name"], "雪" * 83 + " copy")
        self.assertFalse(p["layers"][0]["visible"])
        self.assertEqual(p["layers"][0]["pixels"][1], [8, 9, 10, 0])
        self.assertEqual(p["layers"][1]["pixels"][6], [5, 6, 7, 0])
        q = art.apply_operations(p, [{"op": "clear_layer", "layer": 0}, {"op": "remove_layer", "layer": 1}])
        self.assertEqual(len(q["layers"]), 1)
        self.assertTrue(all(pixel == [0, 0, 0, 0] for pixel in q["layers"][0]["pixels"]))
        self.assertEqual(len(p["layers"]), 2)

    def test_transforms_reject_clipping_of_hidden_transparent_rgb_atomically(self):
        p = art.create_project(2, 2, [
            {"op": "pixel", "layer": 0, "x": 1, "y": 1, "color": [7, 0, 0, 0]},
            {"op": "set_visibility", "layer": 0, "visible": False},
        ])
        before = json.dumps(p)
        for bad in [
            {"op": "translate_layer", "layer": 0, "dx": 1, "dy": 0, "clip": False},
            {"op": "resize_canvas", "width": 1, "height": 2, "clip": False},
            {"op": "resize_canvas", "width": 1, "height": 2, "clip": 1},
            {"op": "translate_layer", "layer": 0, "dx": True, "dy": 0, "clip": True},
            {"op": "flip_layer", "layer": 0, "axis": "diagonal"},
            {"op": "remove_layer", "layer": 0},
            {"op": "move_layer", "from": 0, "to": 1},
        ]:
            with self.assertRaises(ValueError):
                art.apply_operations(p, [{"op": "add_layer"}, {"op": "remove_layer", "layer": 1}, bad])
            self.assertEqual(json.dumps(p), before)
        q = art.apply_operations(p, [{"op": "resize_canvas", "width": 1, "height": 1, "clip": True}])
        self.assertEqual(q["layers"][0]["pixels"], [[0, 0, 0, 0]])

    def test_shape_validation_and_batch_failure_are_atomic(self):
        p = art.create_project(4, 4, [])
        before = json.dumps(p)
        for bad in [
            {"op": "ellipse", "layer": 0, "x": 0, "y": 0, "width": 2, "height": 2, "filled": 1, "color": [1, 2, 3, 4]},
            {"op": "brush_line", "layer": 0, "from": [0, 0], "to": [1, 1], "size": 17, "color": [1, 2, 3, 4]},
            {"op": "rectangle_outline", "layer": 0, "x": 3, "y": 3, "width": 2, "height": 1, "color": [1, 2, 3, 4]},
        ]:
            with self.assertRaises(ValueError):
                art.apply_operations(p, [{"op": "pixel", "layer": 0, "x": 0, "y": 0, "color": [5, 6, 7, 8]}, bad])
            self.assertEqual(json.dumps(p), before)

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
