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

    def test_bad_configuration_never_prints_raw_details(self):
        result = subprocess.run(["python3", "-I", "-c", SCRIPT.read_text(), "private-invalid-marker"],
            input=b"", stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=3)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(json.loads(result.stderr), {"probe_failed": True, "stage": "configuration"})
        self.assertNotIn(b"private-invalid-marker", result.stdout + result.stderr)


if __name__ == "__main__":
    unittest.main()
