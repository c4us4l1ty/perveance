"""Protocol/loop tests on ordinary fixture files; NOT cgroup enforcement tests.
Real kernel cgroup and descendant tests live in tests/bounded_worker.rs.
"""
import importlib.util
import os
from pathlib import Path
import select
import subprocess
import sys
import tempfile
import time
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / 'src' / 'bounded_worker' / 'watchdog.py'
spec = importlib.util.spec_from_file_location('perveance_watchdog', SCRIPT)
watchdog = importlib.util.module_from_spec(spec)
spec.loader.exec_module(watchdog)


@unittest.skipUnless(sys.platform == 'linux', 'Linux descriptor protocol')
class WatchdogTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.write('cgroup.events', 'populated 0\nfrozen 0\n')
        self.write('cpu.stat', 'usage_usec 0\nnr_periods 0\n')
        self.write('cgroup.kill', '0')
        self.directory = os.open(self.root, os.O_RDONLY | os.O_DIRECTORY | os.O_CLOEXEC)
        self.children = []

    def tearDown(self):
        for child in self.children:
            if child.stdin and not child.stdin.closed:
                child.stdin.close()
            try:
                child.wait(timeout=4)
            except subprocess.TimeoutExpired:
                child.kill()
                child.wait()
            child.stderr.close()
        os.close(self.directory)
        self.temp.cleanup()

    def write(self, name, text):
        (self.root / name).write_text(text, encoding='ascii')

    def launch(self, wall=2, cpu=1_000_000, expect_ready=True):
        child = subprocess.Popen([sys.executable, '-I', str(SCRIPT), str(wall), str(cpu), str(self.directory)],
                                 stdin=subprocess.PIPE, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE,
                                 pass_fds=(self.directory,), env={})
        self.children.append(child)
        if expect_ready:
            self.assertEqual(self.byte(child), b'R')
        return child

    def byte(self, child, timeout=3):
        readable, _, _ = select.select([child.stderr], [], [], timeout)
        self.assertTrue(readable, 'watchdog protocol did not become readable')
        return os.read(child.stderr.fileno(), 1)

    def assert_killed(self):
        deadline = time.monotonic() + 2
        while time.monotonic() < deadline:
            if (self.root / 'cgroup.kill').read_text() == '1':
                return
            time.sleep(0.01)
        self.fail('fixed watchdog did not issue a kill')

    def test_eof_revokes_the_lease_and_exits_only_after_empty(self):
        child = self.launch()
        child.stdin.close()
        self.assertEqual(child.wait(timeout=2), 0)
        self.assert_killed()
        self.assertEqual(child.stderr.read(), b'')

    def test_any_control_byte_revokes_without_a_command_language(self):
        child = self.launch()
        child.stdin.write(b'x')
        child.stdin.flush()
        self.assertEqual(child.wait(timeout=2), 0)
        self.assert_killed()

    def test_cpu_stop_is_bounded_and_repeats_for_late_arrivals(self):
        child = self.launch()
        self.write('cpu.stat', 'usage_usec 1000000\n')
        self.assertEqual(self.byte(child), b'C')
        self.assert_killed()
        self.write('cgroup.kill', '0')
        self.assert_killed()
        self.assertIsNone(child.poll(), 'timeout watchdog must stay alive until lease revocation')
        child.stdin.close()
        self.assertEqual(child.wait(timeout=2), 0)
        self.assertEqual(child.stderr.read(), b'')

    def test_wall_stop_does_not_require_parent_polling_or_exit(self):
        child = self.launch(wall=1)
        self.assertEqual(self.byte(child), b'T')
        self.assert_killed()
        self.assertIsNone(child.poll())
        child.stdin.close()
        self.assertEqual(child.wait(timeout=2), 0)
        self.assertEqual(child.stderr.read(), b'')

    def test_empty_confirmation_is_required_after_supervisor_exit(self):
        child = self.launch()
        self.write('cgroup.events', 'populated 1\n')
        child.stdin.close()
        self.assert_killed()
        self.assertIsNone(child.poll())
        self.write('cgroup.events', 'populated 0\n')
        self.assertEqual(child.wait(timeout=2), 0)

    def test_corrupt_counter_fails_without_leaking_raw_content(self):
        child = self.launch()
        self.write('cpu.stat', 'artwork_or_credential_sentinel not-a-counter\n')
        self.assertEqual(child.wait(timeout=2), 70)
        self.assert_killed()
        self.assertEqual(child.stderr.read(), b'')

    def test_oversized_or_symlink_control_cannot_be_consumed(self):
        self.write('cpu.stat', 'x' * 4097)
        with self.assertRaises(ValueError):
            watchdog.read_control(self.directory, 'cpu.stat')
        (self.root / 'cpu.stat').unlink()
        (self.root / 'cpu.stat').symlink_to(self.root / 'cgroup.events')
        with self.assertRaises(OSError):
            watchdog.read_control(self.directory, 'cpu.stat')

    def test_counter_parser_rejects_duplicates_overflow_and_signed_values(self):
        for text in ('usage_usec 1\nusage_usec 2\n', 'usage_usec -1', 'usage_usec +1',
                     'usage_usec 18446744073709551616', 'usage_usec 1 extra'):
            self.write('cpu.stat', text)
            with self.assertRaises(ValueError):
                watchdog.counter(self.directory, 'cpu.stat', 'usage_usec')

    def test_bad_limits_never_acknowledge_readiness(self):
        for wall, cpu in ((0, 1_000_000), (31, 1_000_000), (1, 0), (1, 31_000_000)):
            child = self.launch(wall, cpu, expect_ready=False)
            self.assertEqual(child.wait(timeout=2), 70)
            self.assertEqual(child.stderr.read(), b'')


if __name__ == '__main__':
    unittest.main()
