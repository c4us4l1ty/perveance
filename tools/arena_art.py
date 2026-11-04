#!/usr/bin/env python3
"""Limited offline Perveance project/PNG bridge for external agents.

No model, network, sandbox, plugins or arbitrary code are executed. Input paths
and destination directories must be trusted. Not the full Rust editor/CLI.
"""
import copy
import json
import os
from pathlib import Path
import struct
import sys
import tempfile
import zlib

MAX_DIMENSION = 128
MAX_LAYERS = 16
MAX_OPERATIONS = 1024
MAX_BATCH_BYTES = 256 * 1024
MAX_PROJECT_BYTES = 8 * 1024 * 1024


def integer(value, minimum, maximum):
    if type(value) is not int or not minimum <= value <= maximum:
        raise ValueError("invalid integer or out of bounds")
    return value


def color(value):
    if not isinstance(value, list) or len(value) != 4:
        raise ValueError("color must be four RGBA bytes")
    return [integer(v, 0, 255) for v in value]


def exact(value, fields):
    if not isinstance(value, dict) or set(value) != set(fields):
        raise ValueError("unknown or missing fields")


def pairs_no_duplicates(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError("duplicate JSON field")
        result[key] = value
    return result


def load_json(path, limit):
    with open(path, "rb") as f:
        data = f.read(limit + 1)
    if len(data) > limit:
        raise ValueError("input exceeds size limit")
    return json.loads(data.decode("utf-8"), object_pairs_hook=pairs_no_duplicates,
                      parse_constant=lambda _: (_ for _ in ()).throw(ValueError("non-finite JSON number")))


def validate_project(project):
    exact(project, ("version", "width", "height", "layers"))
    if type(project["version"]) is not int or project["version"] != 1:
        raise ValueError("unsupported project version")
    w = integer(project["width"], 1, MAX_DIMENSION)
    h = integer(project["height"], 1, MAX_DIMENSION)
    layers = project["layers"]
    if not isinstance(layers, list) or not 1 <= len(layers) <= MAX_LAYERS:
        raise ValueError("invalid layer count")
    for layer in layers:
        exact(layer, ("name", "visible", "pixels"))
        if not isinstance(layer["name"], str) or len(layer["name"].encode("utf-8")) > 256:
            raise ValueError("invalid layer name")
        if type(layer["visible"]) is not bool or not isinstance(layer["pixels"], list) or len(layer["pixels"]) != w * h:
            raise ValueError("invalid layer data")
        for pixel in layer["pixels"]:
            color(pixel)
    return project


def create_project(width, height, operations):
    w = integer(width, 1, MAX_DIMENSION)
    h = integer(height, 1, MAX_DIMENSION)
    project = {"version": 1, "width": w, "height": h,
               "layers": [{"name": "Layer 1", "visible": True, "pixels": [[0, 0, 0, 0] for _ in range(w * h)]}]}
    return apply_operations(project, operations)


def apply_operations(project, operations):
    validate_project(project)
    if not isinstance(operations, list) or len(operations) > MAX_OPERATIONS:
        raise ValueError("invalid operation batch")
    # The caller's snapshot stays unchanged even when an operation fails.
    project = copy.deepcopy(project)
    w, h = project["width"], project["height"]
    def new_layer(index):
        return {"name": f"Layer {index}", "visible": True, "pixels": [[0, 0, 0, 0] for _ in range(w * h)]}
    layers = project["layers"]
    for op in operations:
        if not isinstance(op, dict) or not isinstance(op.get("op"), str):
            raise ValueError("invalid operation")
        name = op["op"]
        if name == "add_layer":
            exact(op, ("op",))
            if len(layers) >= MAX_LAYERS:
                raise ValueError("layer limit reached")
            layers.append(new_layer(len(layers) + 1))
            continue
        fields = {"rename_layer": ("op", "layer", "name"),
                  "set_visibility": ("op", "layer", "visible"),
                  "pixel": ("op", "layer", "x", "y", "color"),
                  "line": ("op", "layer", "from", "to", "color"),
                  "rectangle": ("op", "layer", "x", "y", "width", "height", "color"),
                  "fill": ("op", "layer", "x", "y", "color")}
        if name not in fields:
            raise ValueError("operation is not supported by the bridge")
        exact(op, fields[name])
        layer = layers[integer(op["layer"], 0, len(layers) - 1)]
        if name == "rename_layer":
            text = op["name"]
            if not isinstance(text, str) or not text.strip() or len(text.encode("utf-8")) > 256:
                raise ValueError("invalid layer name")
            layer["name"] = text
            continue
        if name == "set_visibility":
            if type(op["visible"]) is not bool:
                raise ValueError("visibility must be boolean")
            layer["visible"] = op["visible"]
            continue
        rgba = color(op["color"])
        pixels = layer["pixels"]
        if name == "line":
            def point(value):
                if not isinstance(value, list) or len(value) != 2:
                    raise ValueError("line endpoint must be a pair")
                return integer(value[0], 0, w - 1), integer(value[1], 0, h - 1)
            x, y = point(op["from"])
            x1, y1 = point(op["to"])
            dx, dy = abs(x1 - x), -abs(y1 - y)
            sx, sy = (1 if x < x1 else -1), (1 if y < y1 else -1)
            error = dx + dy
            while True:
                pixels[y * w + x] = rgba.copy()
                if x == x1 and y == y1:
                    break
                twice = 2 * error
                if twice >= dy:
                    error += dy
                    x += sx
                if twice <= dx:
                    error += dx
                    y += sy
            continue
        x, y = integer(op["x"], 0, w - 1), integer(op["y"], 0, h - 1)
        if name == "pixel":
            pixels[y * w + x] = rgba
        elif name == "rectangle":
            rw = integer(op["width"], 1, w - x)
            rh = integer(op["height"], 1, h - y)
            for row in range(y, y + rh):
                for col in range(x, x + rw):
                    pixels[row * w + col] = rgba.copy()
        else:  # four-connected raw-color fill
            initial = pixels[y * w + x]
            if initial == rgba:
                continue
            start = y * w + x
            pixels[start] = rgba.copy()
            pending = [start]
            while pending:
                i = pending.pop()
                cx, cy = i % w, i // w
                for nx, ny in ((cx - 1, cy), (cx + 1, cy), (cx, cy - 1), (cx, cy + 1)):
                    if 0 <= nx < w and 0 <= ny < h:
                        n = ny * w + nx
                        if pixels[n] == initial:
                            pixels[n] = rgba.copy()
                            pending.append(n)
    return project


def rgba_composite(project):
    validate_project(project)
    count = project["width"] * project["height"]
    out = bytearray(count * 4)
    for layer in project["layers"]:
        if not layer["visible"]:
            continue
        for i, src in enumerate(layer["pixels"]):
            j = i * 4
            sa, da = src[3], out[j + 3]
            alpha = sa * 255 + da * (255 - sa)
            if alpha == 0:
                out[j:j + 4] = b"\0\0\0\0"
                continue
            for c in range(3):
                num = src[c] * sa * 255 + out[j + c] * da * (255 - sa)
                out[j + c] = (num + alpha // 2) // alpha
            out[j + 3] = (alpha + 127) // 255
    return out


def png_bytes(project):
    pixels = rgba_composite(project)
    w, h = project["width"], project["height"]
    raw = b"".join(b"\0" + pixels[y * w * 4:(y + 1) * w * 4] for y in range(h))
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b""))


def write_new(path, data):
    target = Path(path)
    parent = target.parent
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(prefix=".perveance-", dir=parent, delete=False) as f:
            temporary = f.name
            f.write(data)
            f.flush()
            os.fsync(f.fileno())
        # Same-directory atomic create-new; never replace an existing target.
        os.link(temporary, target)
    finally:
        if temporary is not None:
            os.unlink(temporary)


def save_project(output, project):
    data = json.dumps(project, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
    if len(data) > MAX_PROJECT_BYTES:
        raise ValueError("project exceeds bridge size limit")
    write_new(output, data)


def main(args):
    if args == ["capabilities"]:
        return {"ok": True, "bridge": "experimental-python-data-only", "project_version": 1,
                "max_dimension": MAX_DIMENSION, "max_layers": MAX_LAYERS,
                "operations": ["add_layer", "rename_layer", "set_visibility", "pixel", "line", "rectangle", "fill"],
                "commands": ["capabilities", "create", "apply", "inspect", "sample", "export-png"],
                "autonomous_execution": False, "rust_cli_parity": False}
    if len(args) == 5 and args[0] == "create":
        _, w, h, ops_path, output = args
        ops = load_json(ops_path, MAX_BATCH_BYTES)
        project = create_project(int(w), int(h), ops)
        save_project(output, project)
        return {"ok": True, "output": output, "width": project["width"], "height": project["height"], "layers": len(project["layers"])}
    if len(args) == 4 and args[0] == "apply":
        _, input_path, output, ops_path = args
        project = load_json(input_path, MAX_PROJECT_BYTES)
        ops = load_json(ops_path, MAX_BATCH_BYTES)
        project = apply_operations(project, ops)
        save_project(output, project)
        return {"ok": True, "output": output, "applied": len(ops)}
    if len(args) == 2 and args[0] == "inspect":
        project = validate_project(load_json(args[1], MAX_PROJECT_BYTES))
        return {"ok": True, "version": 1, "width": project["width"], "height": project["height"],
                "layers": [{"name": layer["name"], "visible": layer["visible"]} for layer in project["layers"]]}
    if len(args) in (5, 6) and args[0] == "sample":
        project = validate_project(load_json(args[1], MAX_PROJECT_BYTES))
        x = integer(int(args[2]), 0, project["width"] - 1)
        y = integer(int(args[3]), 0, project["height"] - 1)
        index = y * project["width"] + x
        if args[4:] == ["composite"]:
            rgba = list(rgba_composite(project)[index * 4:index * 4 + 4])
        elif len(args) == 6 and args[4] == "layer":
            layer = integer(int(args[5]), 0, len(project["layers"]) - 1)
            rgba = project["layers"][layer]["pixels"][index]
        else:
            raise ValueError("sample source must be composite or layer LAYER")
        return {"ok": True, "x": x, "y": y, "source": args[4], "color": rgba}
    if len(args) == 3 and args[0] == "export-png":
        _, input_path, output = args
        project = load_json(input_path, MAX_PROJECT_BYTES)
        result = png_bytes(project)
        write_new(output, result)
        return {"ok": True, "output": output, "width": project["width"], "height": project["height"]}
    raise ValueError("usage: arena_art.py capabilities | create WIDTH HEIGHT OPERATIONS_JSON OUTPUT | apply INPUT OUTPUT OPERATIONS_JSON | inspect INPUT | sample INPUT X Y composite | sample INPUT X Y layer LAYER | export-png INPUT OUTPUT")


if __name__ == "__main__":
    try:
        print(json.dumps(main(sys.argv[1:]), ensure_ascii=True))
    except (ValueError, OSError, UnicodeError, RecursionError, OverflowError) as exc:
        print(json.dumps({"ok": False, "error": str(exc)}, ensure_ascii=True), file=sys.stderr)
        sys.exit(1)
