"""Fixed offline broker peer. Never executes model text or accepts program input."""
import errno
import fcntl
import json
import os
import socket
import stat
import struct
import sys
import time

STAGE = "configuration"


def denied_file(path):
    try:
        fd = os.open(path, os.O_RDONLY | os.O_NONBLOCK)
    except OSError as exc:
        if exc.errno not in (errno.ENOENT, errno.EACCES, errno.EPERM, errno.ENOTDIR, errno.ESRCH):
            raise RuntimeError("unexpected fixture access error") from None
    else:
        os.close(fd)  # Do not consume even the synthetic secret on a bad boundary.
        raise RuntimeError("host fixture unexpectedly reachable")


def denied_socket(family, address):
    with socket.socket(family, socket.SOCK_STREAM) as peer:
        peer.settimeout(0.5)
        try:
            peer.connect(address)
        except (OSError, TimeoutError):
            return
        raise RuntimeError("host listener unexpectedly reachable")


def exact(peer, count):
    result = bytearray()
    while len(result) < count:
        part = peer.recv(count - len(result))
        if not part:
            raise RuntimeError("broker channel closed")
        result.extend(part)
    return bytes(result)


def send(peer, request):
    data = json.dumps(request, separators=(",", ":")).encode("utf-8")
    if not 0 < len(data) <= 8192:
        raise RuntimeError("fixture request too large")
    peer.sendall(struct.pack("!I", len(data)) + data)


def exchange(peer, request, expected):
    send(peer, request)
    size = struct.unpack("!I", exact(peer, 4))[0]
    if not 0 < size <= 65536:
        raise RuntimeError("oversized broker reply")
    reply = json.loads(exact(peer, size))
    assert set(reply) == {"version", "id", "status", "text"}
    assert reply["version"] == 1 and reply["status"] == expected
    assert reply["id"] == (None if expected == "invalid_request" else request["id"])
    if expected == "completed":
        assert reply["text"] == "Fixed offline broker response; no provider called."
    else:
        assert reply["text"] is None


def run(config):
    global STAGE
    STAGE = "descriptors"
    # listdir's own descriptor has closed before the readlink check.
    for name in os.listdir("/proc/self/fd"):
        if int(name) > 2:
            try:
                os.readlink("/proc/self/fd/" + name)
            except FileNotFoundError:
                continue
            raise RuntimeError("unexpected inherited descriptor")
    STAGE = "channel"
    assert stat.S_ISSOCK(os.fstat(1).st_mode)
    STAGE = "environment"
    assert set(os.environ) <= {"PATH", "LC_CTYPE"}
    assert os.environ["PATH"] == "/usr/bin:/bin"
    STAGE = "input"
    assert fcntl.fcntl(0, fcntl.F_GET_SEALS) & 15 == 15
    try:
        os.write(0, b"changed")
    except OSError as exc:
        assert exc.errno in (errno.EBADF, errno.EPERM)
    else:
        raise RuntimeError("input was writable")
    assert os.listdir("/input") == []
    STAGE = "private-files"
    for path in config["private_files"]:
        denied_file(path)
        denied_file("/proc/1/root" + path)
        denied_file("/proc/" + str(config["host_pid"]) + "/root" + path)
    STAGE = "symlink"
    os.symlink(config["private_files"][0], "/work/host-secret-link")
    denied_file("/work/host-secret-link")
    STAGE = "proc"
    denied_file("/proc/" + str(config["host_pid"]) + "/environ")
    denied_file("/proc/" + str(config["host_pid"]) + "/fd/0")
    STAGE = "host-listeners"
    denied_socket(socket.AF_UNIX, config["host_socket"])
    denied_socket(socket.AF_INET, ("127.0.0.1", config["host_port"]))
    STAGE = "project"
    project = json.load(sys.stdin)
    request = {"version": 1, "id": config["id"], "prompt": "fixed confined offline fixture", "max_output_tokens": 10}
    STAGE = "broker"
    with socket.socket(fileno=1) as peer:
        peer.settimeout(5)
        if config["mode"] != "inspect":
            assert config["mode"] in ("hold-before-dispatch", "hold-in-flight")
            send(peer, request)
            while True:
                time.sleep(60)  # trusted broker/cgroup cancellation fixtures
        bad = dict(request, url="https://not-an-endpoint.invalid")
        exchange(peer, bad, "invalid_request")
        exchange(peer, request, config["first_status"])
        exchange(peer, request, "already_finished")
        changed = dict(request, prompt="changed request must conflict")
        exchange(peer, changed, "conflict")
        other = dict(request, id=("0" * 32 if config["id"] != "0" * 32 else "1" * 32))
        exchange(peer, other, "budget_exhausted")
        summary = {"width": project["width"], "height": project["height"], "layers": len(project["layers"]),
                   "private_files_denied": True, "host_socket_denied": True, "host_tcp_denied": True,
                   "host_proc_denied": True, "descriptors_clean": True, "environment_clean": True,
                   "input_sealed": True, "exchanges_verified": True}
        os.write(2, json.dumps(summary, separators=(",", ":")).encode("utf-8"))


if __name__ == "__main__":
    try:
        run(json.loads(sys.argv[1]))
    except Exception:
        # No tracebacks, request text, paths, credential material or raw errors.
        os.write(2, json.dumps({"probe_failed": True, "stage": STAGE}, separators=(",", ":")).encode("ascii"))
        sys.exit(7)
