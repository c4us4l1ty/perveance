#!/usr/bin/env python3
"""Exercise the supplied real Linux CLI, including one downloaded from a bundle.

No provider, worker, registry, executable payload or network call is used. The
fixture describes an explicitly UNAVAILABLE capability, not a bundled plugin.
"""
import copy
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile


def main():
    if len(sys.argv) != 3:
        raise SystemExit("usage: test_document_lifecycle.py CLI_BINARY OPAQUE_V2_FIXTURE")
    binary = str(Path(sys.argv[1]).resolve(strict=True))
    fixture = Path(sys.argv[2]).read_bytes()
    with tempfile.TemporaryDirectory(prefix="perveance-document-") as temporary:
        base = Path(temporary)
        source, output, proposals, tasks = (base / name for name in ("input", "output", "proposals", "tasks"))
        for directory in (source, output, proposals, tasks):
            directory.mkdir(mode=0o700)

        def write(path, value):
            path.write_bytes(json.dumps(value, separators=(",", ":")).encode())
            path.chmod(0o600)

        def cli(*args, ok=True):
            completed = subprocess.run([binary, *map(str, args)], capture_output=True, timeout=30, cwd=base)
            assert (completed.returncode == 0) == ok, (args[0], completed.returncode, completed.stderr[:500])
            raw = completed.stdout if ok else completed.stderr
            value = json.loads(raw)
            # Existing discovery is a metadata object, not an {ok:...}
            # operation reply. Keep that established CLI schema unchanged.
            if args[0] != "capabilities":
                assert value["ok"] == ok
            if not ok:
                assert completed.stdout == b""
                for secret in (str(base).encode(), b"private-native-name", b"opaque-private-sentinel"):
                    assert secret not in raw
            return value

        def published(result, name):
            report = result["publication"]
            data = (output / name).read_bytes()
            assert report["file"] == name and report["bytes"] == len(data)
            assert report["sha256"] == hashlib.sha256(data).hexdigest()
            assert report["committed"] and report["parent_directory_synced"] and not report["autonomous_execution"]
            assert (output / name).stat().st_mode & 0o777 == 0o600
            return data

        capabilities = cli("capabilities")["extensible_documents"]
        assert capabilities["version"] == 2 and not capabilities["opaque_editing"]
        assert not capabilities["gui_support"] and not capabilities["code_loaded"] and not capabilities["autonomous_execution"]
        cli("new", 2, 1, source / "base.json")
        cli("paint", source / "base.json", source / "raster.json", 0, 1, 0, 9, 8, 7, 0)
        (source / "raster.json").chmod(0o600)
        original_raster = (source / "raster.json").read_bytes()
        native = cli("document-import-raster", source, "raster.json", output, "native.json")
        native_bytes = published(native, "native.json")
        assert native["document"]["rendering"] == "native_raster"
        assert re.fullmatch(r"[0-9a-f]{32}", native["document"]["id"])
        imported_again = cli("document-import-raster", source, "raster.json", output, "native-again.json")
        assert published(imported_again, "native-again.json") == native_bytes
        cli("export-png", source / "raster.json", base / "reference.png")
        exported = cli("document-export-png", output, "native.json", output, "native.png")
        assert published(exported, "native.png") == (base / "reference.png").read_bytes()
        assert (source / "raster.json").read_bytes() == original_raster

        (source / "opaque.json").write_bytes(fixture)
        (source / "opaque.json").chmod(0o600)
        original = json.loads(fixture)
        assert original["layers"][1]["content"]["kind"] == "capability" and not original["layers"][1]["visible"]
        payload = original["layers"][1]["content"]["objects"][0]["payload"]
        assert payload["encoding"] == "hex" and bytes(range(256)) in bytes.fromhex(payload["data"])
        inspected = cli("document-inspect", source, "opaque.json")["document"]
        assert inspected["rendering"] == "blocked_unavailable_capabilities"
        assert len(inspected["unavailable_capabilities"]) == 1
        for flag in ("code_loaded", "registry_consulted", "opaque_payloads_validated", "autonomous_execution"):
            assert not inspected[flag]
        inspected_text = json.dumps(inspected)
        assert payload["data"] not in inspected_text and "private-native-name" not in inspected_text
        before = sorted(p.name for p in output.iterdir())
        cli("document-export-png", source, "opaque.json", output, "blocked.png", ok=False)
        assert sorted(p.name for p in output.iterdir()) == before
        copied = cli("document-copy", source, "opaque.json", output, "opaque-copy.json")
        assert json.loads(published(copied, "opaque-copy.json")) == original
        assert copied["document"]["revision"] == inspected["revision"]

        native_id, opaque_id = (layer["id"] for layer in original["layers"])
        edit = {"version": 1, "base_revision": inspected["revision"], "operations": [
            {"op": "move_layer", "layer_id": native_id, "before": None},
            {"op": "rename_layer", "layer_id": opaque_id, "name": "renamed wrapper only"},
            {"op": "set_visibility", "layer_id": opaque_id, "visible": True},
            {"op": "raster_pixels", "layer_id": native_id, "edits": [{"x": 1, "y": 0, "color": [11, 22, 33, 0]}]},
        ]}
        write(proposals / "edit.json", edit)
        changed = cli("document-apply", source, "opaque.json", output, "edited.json", proposals, "edit.json")
        edited_bytes = published(changed, "edited.json")
        edited = json.loads(edited_bytes)
        assert edited["id"] == original["id"] and edited["layers"][1]["id"] == native_id
        assert edited["layers"][0]["id"] == opaque_id
        assert edited["layers"][0]["content"] == original["layers"][1]["content"]
        assert edited["layers"][0]["visible"] and edited["layers"][0]["name"] == "renamed wrapper only"
        assert edited["layers"][1]["content"]["pixels"] == [[9, 8, 7, 0], [11, 22, 33, 0]]
        assert (source / "opaque.json").read_bytes() == fixture
        reopened = cli("document-inspect", output, "edited.json")["document"]
        assert reopened == changed["document"] and reopened["revision"] != inspected["revision"]
        before = sorted(p.name for p in output.iterdir())
        cli("document-apply", output, "edited.json", output, "stale.json", proposals, "edit.json", ok=False)
        cli("document-export-png", output, "edited.json", output, "still-blocked.png", ok=False)
        cli("document-copy", source, "opaque.json", output, "opaque-copy.json", ok=False)
        assert (output / "edited.json").read_bytes() == edited_bytes
        assert sorted(p.name for p in output.iterdir()) == before

        invalid_edit = copy.deepcopy(edit)
        invalid_edit["operations"] = [
            {"op": "rename_layer", "layer_id": native_id, "name": "must roll back"},
            {"op": "raster_pixels", "layer_id": opaque_id, "edits": [{"x": 0, "y": 0, "color": [1, 2, 3, 4]}]},
        ]
        write(proposals / "invalid.json", invalid_edit)
        cli("document-apply", source, "opaque.json", output, "partial.json", proposals, "invalid.json", ok=False)
        invalid_edit["operations"] = [{"op": "set_object_payload", "payload": "opaque-private-sentinel"}]
        write(proposals / "invalid.json", invalid_edit)
        cli("document-apply", source, "opaque.json", output, "arbitrary.json", proposals, "invalid.json", ok=False)
        assert sorted(p.name for p in output.iterdir()) == before

        for mutate in (
            lambda doc: doc.update(version=3),
            lambda doc: doc.update(command="opaque-private-sentinel"),
            lambda doc: doc["layers"][0].update(id=doc["id"]),
            lambda doc: doc["layers"][1]["content"]["objects"][0]["payload"].update(data="AA"),
            lambda doc: doc["layers"][1]["content"]["objects"][0]["payload"].update(data="00" * (16 * 1024 + 1)),
            lambda doc: doc["layers"][1]["content"]["capability"].update(url="opaque-private-sentinel"),
        ):
            invalid = copy.deepcopy(original)
            mutate(invalid)
            write(source / "invalid.json", invalid)
            cli("document-inspect", source, "invalid.json", ok=False)
        os.symlink("opaque.json", source / "linked.json")
        cli("document-inspect", source, "linked.json", ok=False)
        cli("document-copy", source, "../opaque.json", output, "escape.json", ok=False)
        cli("document-copy", source, "opaque.json", output, "../escape.json", ok=False)
        assert not (base / "escape.json").exists() and sorted(p.name for p in output.iterdir()) == before

        # Old authoring, bridge, GUI project loader and both job backends remain
        # v1-only. A v2 file must be rejected BEFORE a job/output can be created.
        legacy_proposal = {"version": 1, "base_revision": "sha256-json-v1:" + "a" * 64,
                           "operations": [{"op": "pixel", "layer": 0, "x": 0, "y": 0, "color": [1, 2, 3, 4]}]}
        write(proposals / "legacy.json", legacy_proposal)
        for private_root, name in ((source, "opaque.json"), (output, "native.json")):
            cli("inspect", private_root / name, ok=False)
            cli("export-png", private_root / name, output / "legacy.png", ok=False)
            cli("task-create", private_root / name, proposals / "legacy.json", tasks, ok=False)
            cli("worker-task-create", private_root, name, proposals, "legacy.json", tasks, ok=False)
        bridge = Path(__file__).resolve().parent / "arena_art.py"
        rejected = subprocess.run([sys.executable, str(bridge), "inspect", str(source / "opaque.json")],
                                  capture_output=True, timeout=30, cwd=base)
        assert rejected.returncode != 0 and not rejected.stdout
        assert json.loads(rejected.stderr)["ok"] is False
        assert not list(tasks.iterdir())
        assert sorted(p.name for p in output.iterdir()) == before
        assert not any(p.name.startswith(".perveance-") for p in output.iterdir())
    print("Document v2 real-CLI lifecycle passed: native PNG parity, stable-ID edits, exact opaque preservation, blocked hidden-capability export, rollback/stale/no-replace/path/legacy rejection; data only, no plugin or autonomy")


if __name__ == "__main__":
    main()
