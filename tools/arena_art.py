#!/usr/bin/env python3
"""Limited offline Perveance project/PNG bridge for external agents.

No model, network, sandbox, plugins or arbitrary code are executed. Input paths
and destination directories must be trusted. Not the full Rust editor/CLI.
"""
import copy
import hashlib
import json
import os
from pathlib import Path
import struct
import sys
import tempfile
import zlib

MAX_DIMENSION = 128
MAX_LAYERS = 16
MAX_EXPORT_SCALE = 8
MAX_BRUSH_SIZE = 16
MAX_REGION_PIXELS = 4096
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
                  "brush_line": ("op", "layer", "from", "to", "size", "color"),
                  "rectangle": ("op", "layer", "x", "y", "width", "height", "color"),
                  "rectangle_outline": ("op", "layer", "x", "y", "width", "height", "color"),
                  "ellipse": ("op", "layer", "x", "y", "width", "height", "filled", "color"),
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
        if name in ("line", "brush_line"):
            size = integer(op["size"], 1, MAX_BRUSH_SIZE) if name == "brush_line" else 1
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
                left, top = x - size // 2, y - size // 2
                for py in range(max(0, top), min(h, top + size)):
                    for px in range(max(0, left), min(w, left + size)):
                        pixels[py * w + px] = rgba.copy()
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
        elif name in ("rectangle", "rectangle_outline", "ellipse"):
            rw = integer(op["width"], 1, w - x)
            rh = integer(op["height"], 1, h - y)
            if name == "ellipse" and type(op["filled"]) is not bool:
                raise ValueError("filled must be boolean")
            def inside(px, py):
                if px < 0 or py < 0 or px >= rw or py >= rh:
                    return False
                dx, dy = 2 * px + 1 - rw, 2 * py + 1 - rh
                return dx * dx * rh * rh + dy * dy * rw * rw <= rw * rw * rh * rh
            for dy in range(rh):
                for dx in range(rw):
                    if name == "rectangle":
                        selected = True
                    elif name == "rectangle_outline":
                        selected = dx == 0 or dy == 0 or dx + 1 == rw or dy + 1 == rh
                    else:
                        selected = inside(dx, dy) and (op["filled"] or any(not inside(nx, ny) for nx, ny in ((dx - 1, dy), (dx + 1, dy), (dx, dy - 1), (dx, dy + 1))))
                    if selected:
                        pixels[(y + dy) * w + x + dx] = rgba.copy()
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


def read_region(project, x, y, width, height, layer=None):
    validate_project(project)
    x = integer(x, 0, project["width"] - 1)
    y = integer(y, 0, project["height"] - 1)
    width = integer(width, 1, project["width"] - x)
    height = integer(height, 1, project["height"] - y)
    if width * height > MAX_REGION_PIXELS:
        raise ValueError("region exceeds 4096 pixels")
    if layer is None:
        composite = rgba_composite(project)
        pixels = [list(composite[i:i + 4]) for i in range(0, len(composite), 4)]
    else:
        layer = integer(layer, 0, len(project["layers"]) - 1)
        pixels = project["layers"][layer]["pixels"]
    stride = project["width"]
    rows = [pixels[(y + row) * stride + x:(y + row) * stride + x + width] for row in range(height)]
    return {"ok": True, "revision": project_revision(project), "x": x, "y": y, "width": width, "height": height,
            "source": "composite" if layer is None else "layer", "layer": layer, "pixels": rows}


def png_bytes(project, scale=1):
    integer(scale, 1, MAX_EXPORT_SCALE)
    pixels = rgba_composite(project)
    w, h = project["width"], project["height"]
    rows = []
    for y in range(h):
        start = y * w * 4
        row = b"".join(bytes(pixels[start + x * 4:start + x * 4 + 4]) * scale for x in range(w))
        rows.extend([b"\0" + row] * scale)
    raw = b"".join(rows)
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w * scale, h * scale, 8, 6, 0, 0, 0))
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


def project_revision(project):
    validate_project(project)
    # Match Rust's v1 struct field order, regardless of input JSON key order.
    ordered = {"version": project["version"], "width": project["width"], "height": project["height"],
               "layers": [{"name": layer["name"], "visible": layer["visible"], "pixels": layer["pixels"]} for layer in project["layers"]]}
    encoded = json.dumps(ordered, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
    return "sha256-json-v1:" + hashlib.sha256(encoded).hexdigest()


def save_project(output, project):
    data = json.dumps(project, ensure_ascii=False, separators=(",", ":")).encode("utf-8")
    if len(data) > MAX_PROJECT_BYTES:
        raise ValueError("project exceeds bridge size limit")
    write_new(output, data)


def main(args):
    if args == ["capabilities"]:
        return {"ok": True, "bridge": "experimental-python-data-only", "project_version": 1,
                "max_dimension": MAX_DIMENSION, "max_layers": MAX_LAYERS, "max_export_scale": MAX_EXPORT_SCALE,
                "max_brush_size": MAX_BRUSH_SIZE, "max_region_pixels": MAX_REGION_PIXELS,
                "operations": ["add_layer", "rename_layer", "set_visibility", "pixel", "line", "brush_line", "rectangle", "rectangle_outline", "ellipse", "fill"],
                "commands": ["capabilities", "create", "apply", "apply-proposal", "inspect", "sample", "read-region", "export-png", "export-png-scaled"],
                "autonomous_execution": False, "rust_cli_parity": False}
    if len(args) == 5 and args[0] == "create":
        _, w, h, ops_path, output = args
        ops = load_json(ops_path, MAX_BATCH_BYTES)
        project = create_project(int(w), int(h), ops)
        save_project(output, project)
        return {"ok": True, "output": output, "width": project["width"], "height": project["height"], "layers": len(project["layers"]), "revision": project_revision(project)}
    if len(args) == 4 and args[0] == "apply":
        _, input_path, output, ops_path = args
        project = load_json(input_path, MAX_PROJECT_BYTES)
        ops = load_json(ops_path, MAX_BATCH_BYTES)
        project = apply_operations(project, ops)
        save_project(output, project)
        return {"ok": True, "output": output, "applied": len(ops)}
    if len(args) == 4 and args[0] == "apply-proposal":
        _, input_path, output, proposal_path = args
        project = validate_project(load_json(input_path, MAX_PROJECT_BYTES))
        proposal = load_json(proposal_path, MAX_BATCH_BYTES)
        exact(proposal, ("version", "base_revision", "operations"))
        if type(proposal["version"]) is not int or proposal["version"] != 1:
            raise ValueError("unsupported proposal version")
        revision = project_revision(project)
        if proposal["base_revision"] != revision:
            raise ValueError("proposal base revision is stale or invalid")
        edited = apply_operations(project, proposal["operations"])
        save_project(output, edited)
        return {"ok": True, "output": output, "applied": len(proposal["operations"]),
                "base_revision": revision, "revision": project_revision(edited)}
    if len(args) == 2 and args[0] == "inspect":
        project = validate_project(load_json(args[1], MAX_PROJECT_BYTES))
        return {"ok": True, "version": 1, "revision": project_revision(project), "width": project["width"], "height": project["height"],
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
        return {"ok": True, "revision": project_revision(project), "x": x, "y": y, "source": args[4], "color": rgba}
    if len(args) in (7, 8) and args[0] == "read-region":
        project = load_json(args[1], MAX_PROJECT_BYTES)
        x, y, width, height = (int(value) for value in args[2:6])
        if args[6:] == ["composite"]:
            return read_region(project, x, y, width, height)
        if len(args) == 8 and args[6] == "layer":
            return read_region(project, x, y, width, height, int(args[7]))
        raise ValueError("region source must be composite or layer LAYER")
    if len(args) == 3 and args[0] == "export-png":
        _, input_path, output = args
        project = load_json(input_path, MAX_PROJECT_BYTES)
        result = png_bytes(project)
        write_new(output, result)
        return {"ok": True, "output": output, "width": project["width"], "height": project["height"]}
    if len(args) == 4 and args[0] == "export-png-scaled":
        _, input_path, output, factor = args
        scale = integer(int(factor), 1, MAX_EXPORT_SCALE)
        project = load_json(input_path, MAX_PROJECT_BYTES)
        result = png_bytes(project, scale)
        write_new(output, result)
        return {"ok": True, "output": output, "width": project["width"] * scale, "height": project["height"] * scale, "scale": scale}
    raise ValueError("usage: arena_art.py capabilities | create WIDTH HEIGHT OPERATIONS_JSON OUTPUT | apply INPUT OUTPUT OPERATIONS_JSON | apply-proposal INPUT OUTPUT PROPOSAL_JSON | inspect INPUT | sample INPUT X Y composite | sample INPUT X Y layer LAYER | read-region INPUT X Y WIDTH HEIGHT composite | read-region INPUT X Y WIDTH HEIGHT layer LAYER | export-png INPUT OUTPUT | export-png-scaled INPUT OUTPUT SCALE")


if __name__ == "__main__":
    try:
        print(json.dumps(main(sys.argv[1:]), ensure_ascii=True))
    except (ValueError, OSError, UnicodeError, RecursionError, OverflowError) as exc:
        print(json.dumps({"ok": False, "error": str(exc)}, ensure_ascii=True), file=sys.stderr)
        sys.exit(1)
