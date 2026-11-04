#!/usr/bin/env python3
"""Summarize Cargo's resolved metadata for human license review.

This is an inventory, not an SPDX validator, legal opinion, SBOM, or proof of
what was linked or redistributed. Only metadata fields are copied; paths and
registry URLs are intentionally omitted.
"""
import json
import sys


def inventory(metadata):
    packages = metadata["packages"]
    workspace = set(metadata["workspace_members"])
    nodes = metadata["resolve"]["nodes"]
    resolved = {node["id"] for node in nodes}
    by_id = {package["id"]: package for package in packages}
    if (len(by_id) != len(packages) or len(resolved) != len(nodes)
            or not resolved.issubset(by_id) or not workspace.issubset(resolved)):
        raise ValueError("Cargo metadata has duplicate or missing package IDs")
    dependencies = []
    for package_id in resolved - workspace:
        package = by_id[package_id]
        source = package.get("source")
        if source is None:
            source_kind = "path"
        elif not isinstance(source, str):
            raise ValueError("invalid Cargo source field")
        elif source.startswith("registry+") or source.startswith("sparse+"):
            source_kind = "registry"
        elif source.startswith("git+"):
            source_kind = "git"
        else:
            source_kind = "other"
        license_expression = package.get("license")
        if license_expression is not None and not isinstance(license_expression, str):
            raise ValueError("invalid Cargo license field")
        dependencies.append({
            "name": package["name"],
            "version": package["version"],
            "license_expression": license_expression,
            "has_license_file_metadata": bool(package.get("license_file")),
            "source_kind": source_kind,
        })
    dependencies.sort(key=lambda item: (item["name"], item["version"], item["source_kind"], item["license_expression"] or ""))
    return {
        "format_version": 1,
        "scope": "Cargo metadata resolved graph for x86_64-unknown-linux-gnu; may include build and development dependencies and is not an exact linked-binary inventory",
        "limitations": "License expressions and license-file presence are unverified package metadata; system libraries, assets, license texts, obligations and legal review are not covered.",
        "dependency_count": len(dependencies),
        "without_license_expression": sum(not item["license_expression"] for item in dependencies),
        "dependencies": dependencies,
    }


def main():
    if len(sys.argv) != 2:
        raise ValueError("usage: license_inventory.py CARGO_METADATA_JSON")
    with open(sys.argv[1], encoding="utf-8") as stream:
        metadata = json.load(stream)
    print(json.dumps(inventory(metadata), indent=2, ensure_ascii=True))


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, TypeError, KeyError) as error:
        print(f"license inventory: {error}", file=sys.stderr)
        sys.exit(1)
