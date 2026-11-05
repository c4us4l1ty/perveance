#!/usr/bin/env python3
"""Test a real CLI's quarantine-only registry, including a downloaded binary.

The artifact deliberately has unvalidated bytes after a wasm v1 header. This
checks storage and NON-availability, never a working plugin or behavior proof.
"""
import copy
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


def main():
    if len(sys.argv) != 3:
        raise SystemExit("usage: test_registry_lifecycle.py CLI_BINARY OPAQUE_DOCUMENT_FIXTURE")
    binary = str(Path(sys.argv[1]).resolve(strict=True))
    fixture = json.loads(Path(sys.argv[2]).read_bytes())
    with tempfile.TemporaryDirectory(prefix="perveance-quarantine-") as temporary:
        base = Path(temporary)
        source, root, other, output = (base / n for n in ("source", "registry", "other", "output"))
        for path in (source, root, other, output):
            path.mkdir(mode=0o700)

        def write(path, value):
            path.write_bytes(json.dumps(value, separators=(",", ":")).encode())
            path.chmod(0o600)

        def cli(*args, ok=True):
            completed = subprocess.run([binary, *map(str, args)], capture_output=True, timeout=30, cwd=base)
            assert (completed.returncode == 0) == ok, (args[0], completed.returncode, completed.stderr[:500])
            value = json.loads(completed.stdout if ok else completed.stderr)
            if args[0] != "capabilities":
                assert value["ok"] == ok
            if not ok:
                assert not completed.stdout
                assert b"candidate-private-sentinel" not in completed.stderr and str(base).encode() not in completed.stderr
            return value

        def snapshot():
            return {str(p.relative_to(root)): hashlib.sha256(p.read_bytes()).hexdigest()
                    for p in root.rglob("*") if p.is_file()}

        caps = cli("capabilities")["capability_quarantine"]
        assert caps["version"] == 1 and caps["max_entries"] == 32
        for flag in ("activation", "capability_verified", "provenance_verified", "code_loaded", "autonomous_execution"):
            assert not caps[flag]
        initialized = cli("registry-init", root)["initialization"]
        assert initialized["publication"]["committed"] and initialized["publication"]["parent_directory_synced"]
        scope = initialized["registry"]["scope"]
        assert len(scope) == 32 and not initialized["registry"]["entries"]
        before = snapshot()
        cli("registry-init", root, ok=False)
        assert snapshot() == before

        artifact = b"\0asm\x01\0\0\0candidate-private-sentinel\0\xff" + bytes(range(256))
        (source / "module.wasm").write_bytes(artifact)
        (source / "module.wasm").chmod(0o600)
        manifest = {"version": 1,
                    "reference": {"id": "example.quarantined", "version": "1.0.0",
                                  "artifact_sha256": hashlib.sha256(artifact).hexdigest(), "abi_version": 1, "payload_schema": 1},
                    "artifact": {"format": "wasm_v1", "bytes": len(artifact)}, "document_version": 2,
                    "entrypoints": {"validate_payload": "validate_payload_v1", "apply_edits": "apply_edits_v1", "render_layer": "render_layer_v1"},
                    "operations": [{"id": "example_edit", "schema_version": 1}],
                    "runtime": {"fuel": 100000, "memory_bytes": 65536, "output_bytes": 4096},
                    "provenance": {"source_sha256": "a" * 64, "recipe_sha256": "b" * 64}}
        write(source / "manifest.json", manifest)
        document = copy.deepcopy(fixture)
        document["layers"][1]["content"]["capability"] = manifest["reference"]
        write(source / "document.json", document)
        original_document = (source / "document.json").read_bytes()
        revision = cli("document-inspect", source, "document.json")["document"]["revision"]
        missing = cli("registry-resolve-document", root, source, "document.json")["resolution"]
        assert missing["document_revision"] == revision and missing["bindings"][0]["state"] == "missing"

        stored = cli("registry-quarantine", root, source, "manifest.json", "module.wasm")["quarantine"]
        assert stored["admission"] == "quarantined"
        assert stored["registry"]["scope"] == scope and stored["registry"]["total_artifact_bytes"] == len(artifact)
        publication = stored["publication"]
        assert publication["committed"] and publication["parent_directory_synced"] and not publication["autonomous_execution"]
        slot = stored["entry"]["slot"]
        assert len(slot) == 64 and set(slot) <= set("0123456789abcdef")
        package = root / slot
        assert sorted(p.name for p in package.iterdir()) == ["artifact.wasm", "capture.v1.json", "manifest.v1.json"]
        assert (package / "artifact.wasm").read_bytes() == artifact
        for info in publication["artifacts"]:
            raw = (package / info["file"]).read_bytes()
            assert info["bytes"] == len(raw) and info["sha256"] == hashlib.sha256(raw).hexdigest()
            assert (package / info["file"]).stat().st_mode & 0o777 == 0o600
        capture = (package / "capture.v1.json").read_bytes()
        assert b"candidate-private-sentinel" not in capture and str(base).encode() not in capture
        assert json.loads(capture)["purpose"] == "captured_quarantine_only_v1"
        entry = stored["entry"]
        assert entry["state"] == "quarantined_unverified" and entry["storage_integrity_checked"] and entry["wasm_envelope_only_checked"]
        for flag in ("provenance_verified", "capability_verified", "extension_abi_supported", "runtime_limits_enforced", "installed", "active", "available"):
            assert not entry[flag]
        inspected = cli("registry-inspect", root)["registry"]
        assert inspected == stored["registry"] and not inspected["code_loaded"] and not inspected["provider_network_used"] and not inspected["autonomous_execution"]
        assert "candidate-private-sentinel" not in json.dumps(inspected)
        before = snapshot()
        repeated = cli("registry-quarantine", root, source, "manifest.json", "module.wasm")["quarantine"]
        assert repeated["admission"] == "already_quarantined" and repeated["entry"] == entry
        assert snapshot() == before and len(list(root.iterdir())) == 3

        resolved = cli("registry-resolve-document", root, source, "document.json")["resolution"]
        assert resolved["bindings"][0]["state"] == "quarantined_no_runtime" and not resolved["bindings"][0]["available"]
        assert not resolved["code_loaded"] and resolved["document_revision"] == revision
        cli("document-export-png", source, "document.json", output, "blocked.png", ok=False)
        cli("registry-activate", root, slot, ok=False)
        cli("registry-run", root, slot, ok=False)
        assert not list(output.iterdir()) and (source / "document.json").read_bytes() == original_document
        changed_pin = copy.deepcopy(document)
        changed_pin["layers"][1]["content"]["capability"]["payload_schema"] += 1
        write(source / "wrong-pin.json", changed_pin)
        assert cli("registry-resolve-document", root, source, "wrong-pin.json")["resolution"]["bindings"][0]["state"] == "pinned_reference_mismatch"

        rebind = copy.deepcopy(manifest)
        rebind["provenance"]["recipe_sha256"] = "c" * 64
        write(source / "manifest.json", rebind)
        cli("registry-quarantine", root, source, "manifest.json", "module.wasm", ok=False)
        assert snapshot() == before
        next_version = copy.deepcopy(manifest)
        next_version["reference"]["version"] = "2.0.0"
        next_version["reference"]["abi_version"] = 99
        next_version["document_version"] = 3
        write(source / "manifest.json", next_version)
        second = cli("registry-quarantine", root, source, "manifest.json", "module.wasm")["quarantine"]
        assert len(second["registry"]["entries"]) == 2 and not second["entry"]["document_version_supported"]
        future_document = copy.deepcopy(document)
        future_document["layers"][1]["content"]["capability"] = next_version["reference"]
        write(source / "future-document.json", future_document)
        assert cli("registry-resolve-document", root, source, "future-document.json")["resolution"]["bindings"][0]["state"] == "incompatible_document_version"
        assert (package / "artifact.wasm").read_bytes() == artifact

        before = snapshot()
        for field, value in (("verified", True), ("permissions", {"network": True}), ("command", "candidate-private-sentinel"), ("dependencies", ["untrusted"])):
            invalid = copy.deepcopy(manifest)
            invalid[field] = value
            write(source / "invalid.json", invalid)
            cli("registry-quarantine", root, source, "invalid.json", "module.wasm", ok=False)
        os.symlink("module.wasm", source / "linked.wasm")
        write(source / "manifest.json", manifest)
        cli("registry-quarantine", root, source, "manifest.json", "linked.wasm", ok=False)
        cli("registry-quarantine", root, source, "../manifest.json", "module.wasm", ok=False)
        (source / "module.wasm").write_bytes(b"changed source digest")
        cli("registry-quarantine", root, source, "manifest.json", "module.wasm", ok=False)
        (source / "module.wasm").write_bytes(artifact)
        assert snapshot() == before

        cli("registry-init", other)
        shutil.copytree(package, other / slot)
        cli("registry-inspect", other, ok=False)  # Capture belongs to another registry scope.

        stage = root / (".perveance-publish-" + "0" * 32)
        stage.mkdir(mode=0o700)
        (stage / "partial").write_bytes(b"retain uncertain data")
        interrupted = cli("registry-inspect", root)["registry"]
        assert interrupted["mutations_blocked"] and interrupted["interrupted_stages"] == 1 and len(interrupted["entries"]) == 2
        cli("registry-quarantine", root, source, "manifest.json", "module.wasm", ok=False)
        assert (stage / "partial").read_bytes() == b"retain uncertain data"
        assert cli("registry-resolve-document", root, source, "document.json")["resolution"]["mutations_blocked"]
        corrupted = bytearray(artifact)
        corrupted[8] ^= 1
        (package / "artifact.wasm").write_bytes(corrupted)
        cli("registry-inspect", root, ok=False)
        assert (stage / "partial").exists() and (package / "artifact.wasm").read_bytes() == corrupted
    print("Quarantine registry real-CLI lifecycle passed: immutable version/pin binding, exact storage deduplication/reopen, document diagnostics, corruption/path/self-approval denial, private atomic publication and interrupted-stage retention; NO verification, install, execution or autonomy")


if __name__ == "__main__":
    main()
