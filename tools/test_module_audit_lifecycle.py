#!/usr/bin/env python3
"""Real debug/downloaded CLI static diagnostics; no WASM code is executed.

Hand-encoded passing modules are stubs (or an infinite loop), NOT functioning
creative extensions. Structural/signature success must not authorize anything.
"""
import copy
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


HEADER = b"\0asm\x01\0\0\0"
NO_AUTHORITY = (
    "module_instantiated", "code_executed", "runtime_limits_enforced",
    "behavior_verified", "payload_schemas_verified", "provenance_verified",
    "capability_verified", "activation_permitted", "registry_modified",
    "installed", "active", "available", "autonomous_execution",
)


def uint(value):
    output = bytearray()
    while True:
        digit, value = value & 127, value >> 7
        output.append(digit | (128 if value else 0))
        if not value:
            return bytes(output)


def text(value):
    data = value.encode()
    return uint(len(data)) + data


def export_section(names):
    return uint(len(names)) + b"".join(text(name) + bytes([kind]) + uint(index)
                                      for name, kind, index in names)


def module(*, memory=b"\x01\x01\x01\x01", exports=None, functions=None,
           types=None, imports=None, start=None, custom=None):
    names = [("memory", 2, 0), ("perveance_alloc_v1", 0, 0),
             ("validate_payload_v1", 0, 1), ("apply_edits_v1", 0, 2), ("render_layer_v1", 0, 3)]
    bodies = [b"\0\x41\0\x0b", b"\0\x42\0\x0b", b"\0\x42\0\x0b", b"\0\x42\0\x0b"]
    sections = [(1, types if types is not None else bytes.fromhex("0260027f7f017e60017f017f"))]
    if imports is not None:
        sections.append((2, imports))
    sections.extend([(3, b"\x04\x01\0\0\0"), (5, memory),
                     (7, export_section(exports if exports is not None else names))])
    if start is not None:
        sections.append((8, uint(start)))
    selected = functions if functions is not None else bodies
    sections.append((10, uint(len(selected)) + b"".join(uint(len(b)) + b for b in selected)))
    if custom is not None:
        sections.append((0, custom))
    return HEADER + b"".join(bytes([kind]) + uint(len(data)) + data for kind, data in sections)


def main():
    if len(sys.argv) != 3:
        raise SystemExit("usage: test_module_audit_lifecycle.py CLI_BINARY OPAQUE_DOCUMENT_FIXTURE")
    binary = str(Path(sys.argv[1]).resolve(strict=True))
    fixture = json.loads(Path(sys.argv[2]).read_bytes())
    with tempfile.TemporaryDirectory(prefix="perveance-module-audit-") as temporary:
        base = Path(temporary)
        source, root, other, output = (base / n for n in ("source", "registry", "other", "output"))
        for path in (source, root, other, output):
            path.mkdir(mode=0o700)

        def cli(*args, ok=True):
            p = subprocess.run([binary, *map(str, args)], capture_output=True, timeout=30, cwd=base)
            assert (p.returncode == 0) == ok, (args[0], p.returncode, p.stderr[:500])
            data = p.stdout if ok else p.stderr
            assert b"candidate-private-sentinel" not in data and str(base).encode() not in data
            result = json.loads(data)
            if args[0] != "capabilities":
                assert result["ok"] == ok
            if not ok:
                assert not p.stdout
            return result

        def write(path, data):
            path.write_bytes(data if isinstance(data, bytes) else json.dumps(data, separators=(",", ":")).encode())
            path.chmod(0o600)

        def snapshot():
            return {str(p.relative_to(root)): (p.stat().st_mode & 0o777, hashlib.sha256(p.read_bytes()).hexdigest())
                    for p in root.rglob("*") if p.is_file()}

        caps = cli("capabilities")
        assert "registry-audit-module" in caps["operations"]
        caps = caps["static_module_audit"]
        assert caps["version"] == 1 and caps["diagnostic_only"] and not caps["approval_persisted"]
        assert caps["inspector"] == "wasmparser-0.240.0"
        for flag in ("module_instantiated", "code_executed", "behavior_verified", "payload_schemas_verified",
                     "runtime_limits_enforced", "activation_permitted", "available", "autonomous_execution"):
            assert not caps[flag]
        scope = cli("registry-init", root)["initialization"]["registry"]["scope"]
        template = {"version": 1,
                    "reference": {"id": "example.module_audit", "version": "1.0.0", "artifact_sha256": "",
                                  "abi_version": 1, "payload_schema": 1},
                    "artifact": {"format": "wasm_v1", "bytes": 0}, "document_version": 2,
                    "entrypoints": {"validate_payload": "validate_payload_v1", "apply_edits": "apply_edits_v1", "render_layer": "render_layer_v1"},
                    "operations": [{"id": "example_edit", "schema_version": 1}],
                    "runtime": {"fuel": 100000, "memory_bytes": 65536, "output_bytes": 4096},
                    "provenance": {"source_sha256": "a" * 64, "recipe_sha256": "b" * 64}}

        def capture_and_audit(artifact, patch, *, passed=False, issue=None, future=False):
            manifest = copy.deepcopy(template)
            manifest["reference"].update(version=f"1.0.{patch}", artifact_sha256=hashlib.sha256(artifact).hexdigest())
            manifest["artifact"]["bytes"] = len(artifact)
            if future:
                manifest["reference"]["abi_version"] = 99
            write(source / "manifest.json", manifest)
            write(source / "module.wasm", artifact)
            entry = cli("registry-quarantine", root, source, "manifest.json", "module.wasm")["quarantine"]["entry"]
            before = snapshot()
            result = cli("registry-audit-module", root, entry["slot"])["module_audit"]
            assert result["registry_scope"] == scope and result["slot"] == entry["slot"] and result["storage_integrity_checked"]
            audit = result["audit"]
            assert audit["static_profile_passed"] == passed and audit["reference"] == manifest["reference"]
            assert audit["artifact_sha256"] == manifest["reference"]["artifact_sha256"] and audit["artifact_bytes"] == len(artifact)
            package = root / entry["slot"]
            assert audit["manifest_sha256"] == hashlib.sha256((package / "manifest.v1.json").read_bytes()).hexdigest()
            for flag in NO_AUTHORITY:
                assert not audit[flag], flag
            if passed:
                assert audit["structural_validation"] == "passed" and audit["abi_signatures_match"] and audit["rejection"] is None
            if issue:
                assert audit["rejection"] == issue, audit
            assert snapshot() == before and sorted(p.name for p in package.iterdir()) == ["artifact.wasm", "capture.v1.json", "manifest.v1.json"]
            return manifest, entry, result

        good = module(custom=text("candidate-private-sentinel") + b"not executable host text")
        manifest, first, first_audit = capture_and_audit(good, 0, passed=True)
        # Source mutation does not change the exact frozen registry artifact.
        write(source / "module.wasm", b"source no longer resembles a module")
        before = snapshot()
        assert cli("registry-audit-module", root, first["slot"])["module_audit"] == first_audit
        assert snapshot() == before
        loop = b"\0\x03\x40\x0c\0\x0b\x42\0\x0b"
        stubs = [b"\0\x41\0\x0b", loop, b"\0\x42\0\x0b", b"\0\x42\0\x0b"]
        capture_and_audit(module(functions=stubs), 1, passed=True)  # deliberately nonterminating if called!
        stubs[1] = b"\0\x7c\x0b"
        capture_and_audit(module(functions=stubs), 2, issue="invalid_or_unsupported_wasm")
        capture_and_audit(module(types=bytes.fromhex("0260017f017e60017f017f")), 3, issue="abi_signatures")
        capture_and_audit(module(memory=b"\x01\0\x01"), 4, issue="memory_profile")
        capture_and_audit(module(memory=b"\x01\x01\x01\x02"), 5, issue="memory_profile")
        capture_and_audit(module(start=0), 6, issue="start_forbidden")
        imports = b"\x01" + text("candidate-private-sentinel") + text("read_secret") + b"\0\0"
        capture_and_audit(module(imports=imports), 7, issue="imports_forbidden")
        capture_and_audit(module(exports=[("memory", 2, 0), ("perveance_alloc_v1", 0, 0)]), 8, issue="abi_exports")
        capture_and_audit(module(custom=b"\0" * (64 * 1024 + 1)), 9, issue="resource_limit")
        capture_and_audit(module(), 10, issue="unsupported_manifest_contract", future=True)
        capture_and_audit(HEADER, 11, issue="memory_profile")

        document = copy.deepcopy(fixture)
        document["layers"][1]["content"]["capability"] = manifest["reference"]
        write(source / "document.json", document)
        resolution = cli("registry-resolve-document", root, source, "document.json")["resolution"]
        assert resolution["bindings"][0]["state"] == "quarantined_no_runtime" and not resolution["bindings"][0]["available"]
        cli("document-export-png", source, "document.json", output, "blocked.png", ok=False)
        cli("registry-activate", root, first["slot"], ok=False)
        cli("registry-run", root, first["slot"], ok=False)
        assert not list(output.iterdir())
        inventory = cli("registry-inspect", root)["registry"]
        for entry in inventory["entries"]:
            assert entry["state"] == "quarantined_unverified" and entry["wasm_envelope_only_checked"]
            for flag in ("capability_verified", "provenance_verified", "extension_abi_supported", "runtime_limits_enforced", "installed", "active", "available"):
                assert not entry[flag]
        before = snapshot()
        for slot in ("latest", "../candidate-private-sentinel", "F" * 64, "f" * 64):
            cli("registry-audit-module", root, slot, ok=False)
        assert snapshot() == before
        cli("registry-init", other)
        shutil.copytree(root / first["slot"], other / first["slot"])
        cli("registry-audit-module", other, first["slot"], ok=False)
        # A corrupt *different* package also blocks a positive target's audit.
        different_slot = next(e["slot"] for e in inventory["entries"] if e["slot"] != first["slot"])
        different = root / different_slot / "artifact.wasm"
        original = different.read_bytes()
        write(different, b"corrupt unrelated package")
        cli("registry-audit-module", root, first["slot"], ok=False)
        write(different, original)
        assert cli("registry-audit-module", root, first["slot"])["module_audit"] == first_audit
        stage = root / (".perveance-publish-" + "0" * 32)
        stage.mkdir(mode=0o700)
        write(stage / "partial", b"retain uncertain storage")
        before = snapshot()
        cli("registry-audit-module", root, first["slot"], ok=False)
        assert cli("registry-inspect", root)["registry"]["mutations_blocked"] and snapshot() == before
    print("Static WASM real-CLI lifecycle passed: structural/signature checks, bounded policy denials, frozen exact pins, full inventory/scope/orphan checks, no persisted approval and continued render refusal; NO execution, capability verification, installation, activation or autonomy")


if __name__ == "__main__":
    main()
