"""Embedded fixed workloads for the cgroup backend, not a general code runner."""
import errno
import fcntl
import json
import os
import sys
import time

mode = sys.argv[1]
if mode == "wait":
    time.sleep(60)
elif mode == "fork-tree":
    child = os.fork()
    if child == 0:
        os.setsid()
        os.fork()
    print('{"descendants_ready":true}', flush=True)
    time.sleep(60)
elif mode == "cpu":
    while True:
        pass
elif mode == "memory":
    allocations = []
    while True:
        allocations.append(bytearray(1024 * 1024))
elif mode == "processes":
    for _ in range(64):
        try:
            child = os.fork()
        except OSError as exc:
            if exc.errno != errno.EAGAIN:
                raise
            break
        if child == 0:
            time.sleep(60)
            os._exit(0)
    time.sleep(60)
elif mode == "scratch":
    # Each file stays below the inherited per-file rlimit; only the aggregate
    # tmpfs limit can stop this workload with ENOSPC.
    written = 0
    try:
        for index in range(32768):
            with open('/work/block-' + str(index), 'xb', buffering=0) as output:
                written += output.write(b'x' * 4096)
    except OSError as exc:
        if exc.errno != errno.ENOSPC:
            raise
        print(json.dumps({'scratch_full': True, 'bytes': written}), flush=True)
    else:
        raise RuntimeError('scratch limit was not enforced')
elif mode == "output":
    sys.stdout.write('x' * 100000)
    sys.stdout.flush()
elif mode == "fail":
    sys.exit(7)
elif mode == "inspect":
    # listdir's own descriptor disappears before these readlink calls.
    for entry in os.listdir('/proc/self/fd'):
        if int(entry) > 2:
            try:
                os.readlink('/proc/self/fd/' + entry)
            except FileNotFoundError:
                continue
            raise RuntimeError('unexpected inherited descriptor')
    for target in ('/input/project.perveance.json', '/input/extra', '/extra', '/dev/extra'):
        try:
            with open(target, 'ab'):
                pass
        except OSError as exc:
            if exc.errno not in (errno.EROFS, errno.EACCES, errno.EPERM):
                raise
        else:
            raise RuntimeError('expected read-only path')
    assert os.stat('/work').st_dev == os.stat('/tmp').st_dev == os.stat('/dev/shm').st_dev
    with open('/work/probe', 'xb') as output:
        output.write(b'fixed-only')
    assert open('/tmp/probe', 'rb').read() == b'fixed-only'
    assert open('/dev/shm/probe', 'rb').read() == b'fixed-only'
    # Full input arrives as a kernel-sealed regular stdin, not a host path or
    # directory descriptor. Helpers never copy it under the 8-KiB file rlimit.
    assert fcntl.fcntl(0, fcntl.F_GET_SEALS) & 15 == 15
    try:
        os.write(0, b'changed')
    except OSError as exc:
        assert exc.errno in (errno.EBADF, errno.EPERM)
    else:
        raise RuntimeError('input was writable')
    assert os.listdir('/input') == []
    project = json.load(sys.stdin)
    print(json.dumps({'width': project['width'], 'height': project['height'], 'layers': len(project['layers'])}))
else:
    sys.exit(7)
