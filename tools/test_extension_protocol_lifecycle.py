#!/usr/bin/env python3
"""Actual CLI data-only protocol diagnostics, never a module invocation.

Replies are hand-authored fixtures, not executed code or behavioral evidence.
"""
import copy
import hashlib
import json
from pathlib import Path
import struct
import subprocess
import sys
import tempfile

from test_module_audit_lifecycle import module


def blob(data):
    return struct.pack("<I", len(data)) + data


def packet(call, kind, payload=b"", status=0):
    return b"PVEXRP01" + struct.pack("<BB2x", kind, status) + bytes.fromhex(call["request_sha256"]) + blob(payload)


def main():
    if len(sys.argv) != 3:
        raise SystemExit("usage: test_extension_protocol_lifecycle.py CLI_BINARY OPAQUE_DOCUMENT_FIXTURE")
    binary = str(Path(sys.argv[1]).resolve(strict=True))
    fixture = json.loads(Path(sys.argv[2]).read_bytes())
    with tempfile.TemporaryDirectory(prefix="perveance-protocol-") as temporary:
        base = Path(temporary)
        root, source, output = (base / p for p in ("registry", "source", "output"))
        for path in (root, source, output):
            path.mkdir(mode=0o700)

        def cli(*args, ok=True):
            p = subprocess.run([binary, *map(str, args)], cwd=base, capture_output=True, timeout=30)
            assert (p.returncode == 0) == ok, (args[0], p.returncode, p.stderr[:500])
            raw = p.stdout if ok else p.stderr
            for private in (str(base).encode(), b"opaque-private-sentinel", b"private-native-name", b"candidate-private-sentinel"):
                assert private not in raw, (args[0], private)
            result = json.loads(raw)
            if args[0] != "capabilities":
                assert result["ok"] == ok
            if not ok:
                assert not p.stdout
            return result

        def write(name, data):
            path = source / name
            path.write_bytes(data if isinstance(data, bytes) else json.dumps(data, separators=(",", ":")).encode())
            path.chmod(0o600)

        def unchanged():
            return {str(p.relative_to(base)): hashlib.sha256(p.read_bytes()).hexdigest()
                    for p in root.rglob("*") if p.is_file()}

        caps = cli("capabilities")["extension_protocol"]
        assert caps["version"] == 1 and caps["diagnostic_only"] and caps["max_changes"] == 128
        assert all(flag is False for flag in caps["gates"].values())
        cli("registry-init", root)
        artifact = module()
        manifest = {"version": 1,
                    "reference": {"id": "example.protocol", "version": "1.0.0", "artifact_sha256": hashlib.sha256(artifact).hexdigest(), "abi_version": 1, "payload_schema": 1},
                    "artifact": {"format": "wasm_v1", "bytes": len(artifact)}, "document_version": 2,
                    "entrypoints": {"validate_payload": "validate_payload_v1", "apply_edits": "apply_edits_v1", "render_layer": "render_layer_v1"},
                    "operations": [{"id": "example_edit", "schema_version": 1}],
                    "runtime": {"fuel": 100000, "memory_bytes": 65536, "output_bytes": 4096},
                    "provenance": {"source_sha256": "a" * 64, "recipe_sha256": "b" * 64}}
        write("manifest.json", manifest)
        write("module.wasm", artifact)
        slot = cli("registry-quarantine", root, source, "manifest.json", "module.wasm")["quarantine"]["entry"]["slot"]
        document = copy.deepcopy(fixture)
        document["layers"][1]["content"]["capability"] = manifest["reference"]
        write("document.json", document)
        before_document = (source / "document.json").read_bytes()
        revision = cli("document-inspect", source, "document.json")["document"]["revision"]
        intent = {"version": 1, "call_id": "00000000000000000000000000000001", "base_revision": revision,
                  "layer_id": document["layers"][1]["id"], "kind": "validate_payload", "edit": None}

        def inspect(reply=None, issue=None):
            write("intent.json", intent)
            before = unchanged()
            args = ["registry-inspect-protocol", root, slot, source, "document.json", source, "intent.json"]
            if reply is not None:
                write("reply.bin", reply)
                args += [source, "reply.bin"]
            result = cli(*args)["protocol"]
            assert result["slot"] == slot and result["storage_integrity_checked"]
            assert all(flag is False for flag in result["call"]["gates"].values())
            if reply is not None:
                r = result["reply"]
                assert r["wire_accepted"] == (issue is None), r
                assert r["issue"] == issue
                assert all(flag is False for flag in r["gates"].values())
                if issue:
                    assert r["projected_layer_sha256"] is None and r["rgba_sha256"] is None
            assert unchanged() == before and (source / "document.json").read_bytes() == before_document
            return result

        call = inspect()["call"]
        assert call["reference"] == manifest["reference"] and call["input_objects"] == 1
        assert inspect(packet(call, 1))["reply"]["candidate_status"] == "success"
        # Repeated inspection has no persistent replay protection or authority.
        assert inspect(packet(call, 1))["reply"]["wire_accepted"]
        bad = bytearray(packet(call, 1))
        bad[12] ^= 1
        inspect(bytes(bad), "request_mismatch")
        inspect(packet(call, 1, b"candidate-private-sentinel"), "payload_shape")
        inspect(packet(call, 2), "wrong_kind")
        inspect(packet(call, 1, status=255), "unknown_status")
        inspect(packet(call, 1) + b"\0", "malformed_frame")
        inspect(packet(call, 1, status=2))

        intent["kind"] = "apply_edits"
        intent["edit"] = {"operation": "example_edit", "schema_version": 1,
                          "arguments": {"encoding": "hex", "data": b"candidate-private-sentinel".hex()},
                          "new_object_ids": ["50000000000000000000000000000000"]}
        edit_call = inspect()["call"]
        assert edit_call["request_sha256"] != call["request_sha256"]
        obj = document["layers"][1]["content"]["objects"][0]
        old = bytes.fromhex(obj["payload"]["data"])
        change = b"\x02" + bytes.fromhex(obj["id"]) + hashlib.sha256(old).digest() + blob(b"new opaque fixture")
        result = inspect(packet(edit_call, 2, struct.pack("<I", 1) + change))["reply"]
        assert result["proposed_changes"] == 1 and result["projected_objects"] == 1 and result["projected_layer_sha256"]
        wrong = bytearray(change)
        wrong[22] ^= 1
        inspect(packet(edit_call, 2, struct.pack("<I", 1) + wrong), "object_precondition")
        inspect(packet(edit_call, 2, struct.pack("<I", 2) + change + change), "duplicate_target")
        inspect(packet(edit_call, 2, struct.pack("<I", 129)), "frame_limit")
        inspect(packet(call, 1), "wrong_kind")

        intent["kind"], intent["edit"] = "render_layer", None
        render_call = inspect()["call"]
        pixels = bytes([1, 2, 3, 0, 250, 249, 248, 128])
        rgba = struct.pack("<IIB3x", 2, 1, 1) + blob(pixels)
        result = inspect(packet(render_call, 3, rgba))["reply"]
        assert result["rgba_sha256"] == hashlib.sha256(pixels).hexdigest() and result["rgba_bytes"] == 8
        wrong = struct.pack("<IIB3x", 1, 2, 1) + blob(pixels)
        inspect(packet(render_call, 3, wrong), "render_shape")
        for size in range(48):
            inspect(packet(render_call, 3, rgba)[:size], "malformed_frame")
        call_args = ["registry-inspect-protocol", root, slot, source, "document.json", source, "intent.json"]
        write("intent.json", dict(intent, base_revision="sha256-document-v2:" + "0" * 64))
        cli(*call_args, ok=False)
        write("intent.json", dict(intent, verified=True))
        cli(*call_args, ok=False)
        write("intent.json", intent)
        cli("registry-inspect-protocol", root, "../candidate-private-sentinel", source, "document.json", source, "intent.json", ok=False)
        cli("document-export-png", source, "document.json", output, "blocked.png", ok=False)
        resolved = cli("registry-resolve-document", root, source, "document.json")["resolution"]
        assert resolved["bindings"][0]["state"] == "quarantined_no_runtime" and not resolved["bindings"][0]["available"]
        assert not list(output.iterdir()) and (source / "document.json").read_bytes() == before_document
        # Source changes cannot change frozen artifact bindings.
        write("module.wasm", b"modified source")
        assert inspect()["call"] == render_call
        stage = root / (".perveance-publish-" + "0" * 32)
        stage.mkdir(mode=0o700)
        before = unchanged()
        cli(*call_args, ok=False)
        assert unchanged() == before and stage.exists()
    print("Extension protocol real-CLI lifecycle passed: exact call binding, bounded typed edit/RGBA fixtures, stale/malformed/scope/orphan denial and no mutation or cached approval; NO WASM call, semantic verification, document edit, PNG, activation or autonomy")


if __name__ == "__main__":
    main()
