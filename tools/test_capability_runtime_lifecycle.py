#!/usr/bin/env python3
"""Real-CLI metered interpreter diagnostics for quarantined stub modules.

The four fixtures are hand-built stubs: a zero allocator, a bound-rejecting
reply writer, an infinite loop and an unconditional trap. Passing them proves
engine metering and refusal bookkeeping only, never useful capability behavior,
verification, mutation, activation or PNG publication.
"""
import copy
import hashlib
import json
from pathlib import Path
import struct
import subprocess
import sys
import tempfile


def uleb(value):
    out = bytearray()
    while True:
        digit, value = value & 0x7F, value >> 7
        out.append(digit | (0x80 if value else 0))
        if not value:
            return bytes(out)


def sleb(value):
    out = bytearray()
    while True:
        byte = value & 0x7F
        value >>= 7
        done = (value == 0 and not byte & 0x40) or (value == -1 and byte & 0x40)
        out.append(byte if done else byte | 0x80)
        if done:
            return bytes(out)


def section(tag, body):
    return bytes([tag]) + uleb(len(body)) + body


def text(value):
    return uleb(len(value)) + value.encode()


def reply_bytes():
    return b"PVEXRP01" + bytes([1, 0, 0, 0]) + bytes(32) + struct.pack("<I", 0)


def module(fixture):
    allocator = b"\x41\x00\x0b" if fixture == "zero_allocator" else b"\x41\x80\x04\x0b"
    if fixture == "reply":
        entry = b"\x42" + sleb((64 << 32) | 48) + b"\x0b"
    elif fixture == "loop":
        entry = b"\x03\x40\x0c\x00\x0b"
    elif fixture == "trap":
        entry = b"\x00\x0b"
    else:
        entry = b"\x42\x00\x0b"
    exports = uleb(5)
    for name, kind, index in [("memory", 2, 0), ("perveance_alloc_v1", 0, 0),
                              ("validate_payload_v1", 0, 1), ("apply_edits_v1", 0, 2),
                              ("render_layer_v1", 0, 3)]:
        exports += text(name) + bytes([kind]) + uleb(index)
    out = b"\0asm\x01\0\0\0"
    out += section(1, bytes([2, 0x60, 2, 0x7F, 0x7F, 1, 0x7E, 0x60, 1, 0x7F, 1, 0x7F]))
    out += section(3, bytes([4, 1, 0, 0, 0]))
    out += section(5, bytes([1, 1, 1, 1]))
    out += section(7, exports)
    if fixture == "reply":
        data = reply_bytes()
        out += section(11, uleb(1) + bytes(4) + b"\x41" + sleb(64) + b"\x0b" + uleb(len(data)) + data)
    code = uleb(4)
    for body in (allocator, entry, entry, entry):
        function = b"\x00" + body
        code += uleb(len(function)) + function
    out += section(10, code)
    return out


def main():
    if len(sys.argv) != 3:
        raise SystemExit("usage: test_capability_runtime_lifecycle.py CLI_BINARY OPAQUE_DOCUMENT_FIXTURE")
    binary = str(Path(sys.argv[1]).resolve(strict=True))
    fixture = json.loads(Path(sys.argv[2]).read_bytes())
    with tempfile.TemporaryDirectory(prefix="perveance-runtime-") as temporary:
        base = Path(temporary)
        source, output = base / "source", base / "output"
        for path in (source, output):
            path.mkdir(mode=0o700)

        def cli(*args, ok=True):
            process = subprocess.run([binary, *map(str, args)], cwd=base, capture_output=True, timeout=60)
            assert (process.returncode == 0) == ok, (args[0], process.returncode, process.stderr[:600])
            raw = process.stdout if ok else process.stderr
            for private in (str(base).encode(), b"opaque-private-sentinel", b"private-native-name"):
                assert private not in raw, (args[0], private)
            result = json.loads(raw)
            if args[0] != "capabilities":
                assert result["ok"] == ok
            return result

        def write(name, data):
            path = source / name
            path.write_bytes(data if isinstance(data, bytes) else json.dumps(data, separators=(",", ":")).encode())
            path.chmod(0o600)

        def tree(root):
            return {str(path.relative_to(base)): hashlib.sha256(path.read_bytes()).hexdigest()
                    for path in root.rglob("*") if path.is_file()}

        expected = {"zero_allocator": "allocation_rejected", "reply": "reply_rejected",
                    "loop": "out_of_fuel", "trap": "trapped"}
        no_authority = ("host_imports_linked", "os_confinement_verified", "behavior_verified",
                        "payload_schemas_verified", "provenance_verified", "capability_verified",
                        "activation_permitted", "installed", "active", "available",
                        "document_modified", "registry_modified", "png_published",
                        "provider_network_used", "autonomous_execution", "authenticated",
                        "replay_protection_enforced")
        enabled = ("fuel_metering_enabled", "memory_limit_enabled", "output_limit_enabled",
                   "deterministic_mode_enabled", "eager_translation")
        caps = cli("capabilities")["capability_runtime"]
        assert caps["engine"] == "wasmi-2.0.0" and caps["diagnostic_only"] and caps["host_imports"] is False
        results, slots = {}, {}
        for index, (name, outcome) in enumerate(expected.items()):
            artifact = module(name)
            registry = base / f"registry-{index}"
            registry.mkdir(mode=0o700)
            reference = {"id": f"example.runtime{index}", "version": "1.0.0",
                         "artifact_sha256": hashlib.sha256(artifact).hexdigest(),
                         "abi_version": 1, "payload_schema": 1}
            manifest = {"version": 1, "reference": reference,
                        "artifact": {"format": "wasm_v1", "bytes": len(artifact)},
                        "document_version": 2,
                        "entrypoints": {"validate_payload": "validate_payload_v1",
                                        "apply_edits": "apply_edits_v1", "render_layer": "render_layer_v1"},
                        "operations": [{"id": "example_edit", "schema_version": 1}],
                        "runtime": {"fuel": 100000, "memory_bytes": 65536, "output_bytes": 1048576},
                        "provenance": {"source_sha256": "a" * 64, "recipe_sha256": "b" * 64}}
            write(f"manifest-{index}.json", manifest)
            write(f"module-{index}.wasm", artifact)
            document = copy.deepcopy(fixture)
            document["layers"][1]["content"]["capability"] = reference
            write(f"document-{index}.json", document)
            document_before = (source / f"document-{index}.json").read_bytes()
            revision = cli("document-inspect", source, f"document-{index}.json")["document"]["revision"]
            intent = {"version": 1, "call_id": f"{index + 1:032x}", "base_revision": revision,
                      "layer_id": document["layers"][1]["id"], "kind": "validate_payload", "edit": None}
            write(f"intent-{index}.json", intent)
            cli("registry-init", registry)
            slot = cli("registry-quarantine", registry, source,
                       f"manifest-{index}.json", f"module-{index}.wasm")["quarantine"]["entry"]["slot"]
            slots[name] = slot
            registry_before = tree(registry)
            args = ["registry-invoke-capability", registry, slot, source, f"document-{index}.json",
                    source, f"intent-{index}.json"]
            runtime = cli(*args)["runtime"]
            assert runtime["outcome"] == outcome, (name, runtime["outcome"])
            gates = runtime["gates"]
            assert all(gates[flag] is False for flag in no_authority), (name, gates)
            assert all(gates[flag] is True for flag in enabled), (name, gates)
            assert gates["engine_compiled"] and gates["module_instantiated"] and gates["code_executed"]
            assert runtime["engine"] == "wasmi-2.0.0"
            assert runtime["instance_memory_bytes"] == 65536
            assert runtime["fuel_declared"] == 100000
            if name == "reply":
                reply = runtime["reply"]
                assert reply["wire_accepted"] is False and reply["issue"] == "request_mismatch"
                assert reply["candidate_status"] is None and reply["rgba_sha256"] is None
            else:
                assert runtime["reply"] is None
            if name == "loop":
                assert runtime["fuel_remaining"] is not None and runtime["fuel_remaining"] < 100000
            assert tree(registry) == registry_before, name
            assert (source / f"document-{index}.json").read_bytes() == document_before, name
            repeated = cli(*args)["runtime"]
            assert repeated == runtime, f"{name} invocation was not deterministic"
            write(f"module-{index}.wasm", b"modified source bytes")
            assert cli(*args)["runtime"] == runtime, f"{name} reread the source instead of the frozen slot"
            results[name] = runtime["outcome"]

        document = copy.deepcopy(fixture)
        document["layers"][1]["content"]["capability"] = {
            "id": "example.runtime0", "version": "1.0.0",
            "artifact_sha256": hashlib.sha256(module("zero_allocator")).hexdigest(),
            "abi_version": 1, "payload_schema": 1}
        write("blocked-document.json", document)
        cli("document-export-png", source, "blocked-document.json", output, "blocked.png", ok=False)
        assert not list(output.iterdir())
        stage = base / "registry-0" / (".perveance-publish-" + "0" * 32)
        stage.mkdir(mode=0o700)
        args = ["registry-invoke-capability", base / "registry-0", slots["zero_allocator"],
                source, "document-0.json", source, "intent-0.json"]
        before = tree(base / "registry-0")
        cli(*args, ok=False)
        assert tree(base / "registry-0") == before and stage.exists()
    print("Metered interpreter lifecycle passed for zero allocator/reply/loop/trap stubs: "
          + ", ".join(f"{name}={outcome}" for name, outcome in results.items())
          + "; frozen-slot determinism, unchanged storage/document, blocked PNG and all authority gates false; NO useful behavior, verification, mutation, activation or autonomy")


if __name__ == "__main__":
    main()
