"""Embedded trusted watchdog. Fixed protocol; never executes worker-supplied code.

FD 0: private supervisor lease pipe (EOF means supervisor exit).
FD 1: /dev/null. A single explicit extra FD holds the managed cgroup directory.
FD 2: bounded protocol bytes: R (ready), then at most one C/T (CPU/time stop).
The process lives outside the worker cgroup and has no provider credentials.
"""
import os
import select
import sys
import time

CONTROL_LIMIT = 4096


def read_control(directory, name):
    fd = os.open(name, os.O_RDONLY | os.O_CLOEXEC | os.O_NOFOLLOW, dir_fd=directory)
    try:
        data = os.read(fd, CONTROL_LIMIT + 1)
        if len(data) > CONTROL_LIMIT:
            raise ValueError("oversized cgroup control")
        return data.decode("ascii")
    finally:
        os.close(fd)


def counter(directory, name, key):
    values = {}
    for line in read_control(directory, name).splitlines():
        fields = line.split()
        if len(fields) != 2 or fields[0] in values or not fields[1].isascii() or not fields[1].isdigit():
            raise ValueError("invalid cgroup counter")
        value = int(fields[1])
        if value > 2**64 - 1:
            raise ValueError("cgroup counter overflow")
        values[fields[0]] = value
    return values[key]


def supervise(wall_seconds, cpu_microseconds, directory):
    kill = None
    try:
        if not 1 <= wall_seconds <= 30 or not 1_000_000 <= cpu_microseconds <= 30_000_000 or directory < 3:
            return 70
        kill = os.open("cgroup.kill", os.O_WRONLY | os.O_CLOEXEC | os.O_NOFOLLOW, dir_fd=directory)
        # Validate everything needed by the loop before the caller can launch.
        if counter(directory, "cgroup.events", "populated") != 0:
            return 70
        counter(directory, "cpu.stat", "usage_usec")
        started = time.monotonic()
        os.write(2, b"R")
        reason = None
        closed = False
        cleanup_started = None
        while True:
            if closed:
                time.sleep(0.02)
                readable = []
            else:
                readable, _, _ = select.select([0], [], [], 0.02)
            if readable:
                # No command language: EOF or any byte both revoke the lease.
                os.read(0, 1)
                closed = True
            now = time.monotonic()
            if reason is None:
                if counter(directory, "cpu.stat", "usage_usec") >= cpu_microseconds:
                    reason = b"C"
                elif now - started >= wall_seconds:
                    reason = b"T"
                if reason is not None:
                    os.write(2, reason)
            if reason is not None or closed:
                # Repeat while the supervisor holds the pipe. This closes the
                # timeout-vs-late-launch race; a one-shot kill would not do so.
                os.lseek(kill, 0, os.SEEK_SET)
                if os.write(kill, b"1") != 1:
                    raise OSError("incomplete cgroup kill")
            if closed:
                if cleanup_started is None:
                    cleanup_started = now
                populated = counter(directory, "cgroup.events", "populated")
                if populated == 0:
                    return 0
                if populated != 1 or now - cleanup_started >= 3:
                    return 71
    except (OSError, ValueError, KeyError, UnicodeError, OverflowError):
        # No paths, environment, artwork or raw exception text in the protocol.
        return 70
    finally:
        if kill is not None:
            try:
                os.lseek(kill, 0, os.SEEK_SET)
                os.write(kill, b"1")
            except OSError:
                pass
            os.close(kill)


def main():
    try:
        if len(sys.argv) != 4:
            return 70
        return supervise(int(sys.argv[1]), int(sys.argv[2]), int(sys.argv[3]))
    except (ValueError, OverflowError):
        return 70


if __name__ == "__main__":
    # Avoid Python shutdown output, handlers or buffered diagnostic output.
    os._exit(main())
