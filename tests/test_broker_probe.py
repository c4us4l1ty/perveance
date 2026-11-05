import importlib.util
import json
from pathlib import Path
import socket
import struct
import subprocess
import tempfile
import threading
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / "src/bounded_worker/broker_probe.py"
spec = importlib.util.spec_from_file_location("fixed_broker_probe", SCRIPT)
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)


class BrokerProbeTests(unittest.TestCase):
    def test_private_file_negative_control_detects_access_without_reading_or_echoing(self):
        with tempfile.TemporaryDirectory() as root:
            path = Path(root) / "secret"
            path.write_bytes(b"synthetic-private-marker")
            with self.assertRaisesRegex(RuntimeError, "unexpectedly reachable") as result:
                probe.denied_file(str(path))
            self.assertNotIn("synthetic-private-marker", str(result.exception))
            self.assertEqual(path.read_bytes(), b"synthetic-private-marker")
            probe.denied_file(str(Path(root) / "absent"))

    def test_exposed_host_socket_negative_control_fails_not_passes(self):
        with tempfile.TemporaryDirectory() as root:
            path = str(Path(root) / "socket")
            with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as listener:
                listener.bind(path)
                listener.listen()
                with self.assertRaisesRegex(RuntimeError, "unexpectedly reachable"):
                    probe.denied_socket(socket.AF_UNIX, path)

    def test_exposed_tcp_listener_negative_control_fails(self):
        with socket.socket() as listener:
            listener.bind(("127.0.0.1", 0))
            listener.listen()
            with self.assertRaisesRegex(RuntimeError, "unexpectedly reachable"):
                probe.denied_socket(socket.AF_INET, listener.getsockname())

    def test_exchange_rejects_bad_binding_status_and_oversized_frame(self):
        request = {"version": 1, "id": "0" * 32, "prompt": "fixed", "max_output_tokens": 10}
        for response in [
            struct.pack("!I", 65537),
            json.dumps({"version": 2, "id": request["id"], "status": "already_finished", "text": None}).encode(),
            json.dumps({"version": 1, "id": "1" * 32, "status": "already_finished", "text": None}).encode(),
            json.dumps({"version": 1, "id": request["id"], "status": "completed", "text": "bad"}).encode(),
        ]:
            server, client = socket.socketpair()
            server.settimeout(2)
            client.settimeout(2)
            def reply():
                with server:
                    size = struct.unpack("!I", probe.exact(server, 4))[0]
                    probe.exact(server, size)
                    server.sendall(response if len(response) == 4 else struct.pack("!I", len(response)) + response)
            worker = threading.Thread(target=reply)
            worker.start()
            with client, self.assertRaises((AssertionError, RuntimeError)):
                probe.exchange(client, request, "already_finished")
            worker.join(2)
            self.assertFalse(worker.is_alive())

    def test_fragmented_bounded_exchange_does_not_evaluate_response_text(self):
        request = {"version": 1, "id": "0" * 32, "prompt": "fixed", "max_output_tokens": 10}
        server, client = socket.socketpair()
        server.settimeout(2)
        client.settimeout(2)
        def reply():
            with server:
                size = struct.unpack("!I", probe.exact(server, 4))[0]
                parsed = json.loads(probe.exact(server, size))
                self.assertEqual(parsed, request)
                response = json.dumps({"version": 1, "id": request["id"], "status": "completed",
                    "text": "Fixed offline broker response; no provider called."}).encode()
                for byte in struct.pack("!I", len(response)) + response:
                    server.sendall(bytes([byte]))
        worker = threading.Thread(target=reply)
        worker.start()
        with client:
            probe.exchange(client, request, "completed")
        worker.join(2)
        self.assertFalse(worker.is_alive())

    def test_runtime_pwd_is_fixed_but_host_environment_is_still_rejected(self):
        clean = {"PATH": "/usr/bin:/bin", "PWD": "/work"}
        probe.verify_environment(clean)
        probe.verify_environment(dict(clean, LC_CTYPE="C.UTF-8"))
        for bad in [dict(clean, PERVEANCE_SYNTHETIC_HOST_SECRET="never-inherit"),
                    dict(clean, OPENROUTER_API_KEY="synthetic-not-real"),
                    dict(clean, PWD="/home/host"), dict(clean, PATH="/host/bin")]:
            with self.assertRaises(AssertionError):
                probe.verify_environment(bad)
        with self.assertRaises(KeyError):
            probe.verify_environment({"PATH": "/usr/bin:/bin"})

    def test_fixed_pixel_candidate_preserves_hidden_data_and_applies_ordered_rgba(self):
        project = {"version": 1, "width": 2, "height": 1, "layers": [
            {"name": "private hidden name", "visible": False, "pixels": [[9, 8, 7, 0], [1, 2, 3, 4]]}]}
        proposal = {"version": 1, "base_revision": "host-checked", "operations": [
            {"op": "pixel", "layer": 0, "x": 1, "y": 0, "color": [10, 20, 30, 128]},
            {"op": "pixel", "layer": 0, "x": 1, "y": 0, "color": [7, 8, 9, 255]}]}
        result = probe.pixel_candidate({"version": 1, "project": project, "proposal": proposal})
        self.assertEqual(result["layers"][0]["pixels"], [[9, 8, 7, 0], [7, 8, 9, 255]])
        self.assertEqual(result["layers"][0]["name"], "private hidden name")
        self.assertFalse(result["layers"][0]["visible"])

    def test_fixed_pixel_candidate_rejects_nonpixel_extra_fields_bad_coordinates_and_booleans(self):
        import copy
        operation = {"op": "pixel", "layer": 0, "x": 0, "y": 0, "color": [1, 2, 3, 4]}
        for bad in [dict(operation, op="shell"), dict(operation, command="ignored-is-not-safe"),
                    dict(operation, x=-1), dict(operation, y=1), dict(operation, layer=True),
                    dict(operation, color=[True, 2, 3, 4]), dict(operation, color=[256, 2, 3, 4])]:
            payload = {"version": 1, "project": {"width": 1, "height": 1, "layers": [{"pixels": [[0, 0, 0, 0]]}]},
                       "proposal": {"version": 1, "base_revision": "host-checked", "operations": [bad]}}
            with self.assertRaises(AssertionError):
                probe.pixel_candidate(copy.deepcopy(payload))

    def test_bad_configuration_never_prints_raw_details(self):
        result = subprocess.run(["python3", "-I", "-c", SCRIPT.read_text(), "private-invalid-marker"],
            input=b"", stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=3)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(json.loads(result.stderr), {"probe_failed": True, "stage": "configuration"})
        self.assertNotIn(b"private-invalid-marker", result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
