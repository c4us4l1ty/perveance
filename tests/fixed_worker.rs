use perveance::{fixed_worker::{run_fixed_worker, FixedMode, FixedOutcome}, Project};
use std::{sync::atomic::AtomicBool, time::Duration};

#[cfg(target_os = "linux")]
#[test]
fn invalid_timeouts_and_pre_cancel_never_launch_or_stage() {
    let parent = tempfile::tempdir().unwrap();
    let p = Project::new(1, 1).unwrap();
    let no = AtomicBool::new(false);
    assert!(run_fixed_worker(&p, parent.path(), FixedMode::Inspect, Duration::ZERO, &no).is_err());
    assert!(run_fixed_worker(&p, parent.path(), FixedMode::Inspect, Duration::from_secs(31), &no).is_err());
    let cancel = AtomicBool::new(true);
    assert_eq!(run_fixed_worker(&p, parent.path(), FixedMode::Inspect, Duration::from_secs(1), &cancel).unwrap(), FixedOutcome::Cancelled);
    assert_eq!(std::fs::read_dir(parent.path()).unwrap().count(), 0);
}

#[cfg(target_os = "linux")]
#[test]
#[ignore = "requires a compatible unprivileged bubblewrap host"]
fn fixed_worker_lifecycle_on_supported_host() {
    use std::{sync::{Arc, atomic::Ordering}, thread};
    let parent = tempfile::tempdir().unwrap();
    let mut p = Project::new(2, 3).unwrap();
    p.layers[0].visible = false;
    p.set_pixel(0, 0, 0, [4, 5, 6, 0]).unwrap();
    // A deliberately inheritable synthetic host file must not survive into the
    // fixed worker. First prove a plain child really can inherit this FD.
    use std::{fs::File, os::fd::AsRawFd, process::Command};
    let secret = parent.path().join("host-secret");
    std::fs::write(&secret, b"synthetic-host-only").unwrap();
    let file = File::open(&secret).unwrap();
    let fd = file.as_raw_fd();
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
    assert!(flags >= 0);
    assert_eq!(unsafe { libc::fcntl(fd, libc::F_SETFD, flags & !libc::FD_CLOEXEC) }, 0);
    let control = Command::new("/usr/bin/python3").args(["-I", "-c", "import os,sys; os.fstat(int(sys.argv[1]))", &fd.to_string()]).status().unwrap();
    assert!(control.success());
    // Characterize the helper without the launcher's FD hygiene. This negative
    // control contains only a disposable file; either observation is recorded
    // because future bubblewrap releases may themselves close the descriptor.
    let mut raw = Command::new("/usr/bin/bwrap");
    raw.args(["--unshare-user", "--unshare-pid", "--unshare-net", "--unshare-ipc", "--unshare-uts", "--new-session", "--die-with-parent", "--clearenv"]);
    for path in ["/usr", "/bin", "/lib", "/lib64"] {
        if std::path::Path::new(path).exists() { raw.args(["--ro-bind", path, path]); }
    }
    let result = raw.args(["--proc", "/proc", "--dev", "/dev", "--tmpfs", "/tmp", "--chdir", "/tmp", "/usr/bin/python3", "-I", "-c",
        "import os,sys\ntry:\n os.fstat(int(sys.argv[1])); print('inherited')\nexcept OSError:\n print('closed')", &fd.to_string()])
        .env_clear().output().unwrap();
    assert!(result.status.success(), "{}", String::from_utf8_lossy(&result.stderr));
    assert!(result.stdout == b"inherited\n" || result.stdout == b"closed\n");
    println!("bare bubblewrap synthetic descriptor: {}", String::from_utf8_lossy(&result.stdout).trim());
    let no = AtomicBool::new(false);
    let inspect = run_fixed_worker(&p, parent.path(), FixedMode::Inspect, Duration::from_secs(10), &no).unwrap();
    assert_eq!(unsafe { libc::fcntl(fd, libc::F_SETFD, flags) }, 0);
    assert_eq!(inspect, FixedOutcome::Completed);
    assert_eq!(std::fs::read_dir(parent.path()).unwrap().count(), 1);
    drop(file);
    std::fs::remove_file(&secret).unwrap();
    for (mode, expected, timeout) in [
        (FixedMode::Inspect, FixedOutcome::Completed, 10),
        (FixedMode::Fail, FixedOutcome::Failed, 10),
        (FixedMode::Flood, FixedOutcome::OutputLimit, 10),
        (FixedMode::Wait, FixedOutcome::TimedOut, 2),
    ] {
        let result = run_fixed_worker(&p, parent.path(), mode, Duration::from_secs(timeout), &no).unwrap();
        assert_eq!(result, expected, "mode {mode:?}");
        assert_eq!(std::fs::read_dir(parent.path()).unwrap().count(), 0, "staging leak after {mode:?}");
    }
    let cancel = Arc::new(AtomicBool::new(false));
    let signal = Arc::clone(&cancel);
    let setter = thread::spawn(move || { thread::sleep(Duration::from_millis(700)); signal.store(true, Ordering::SeqCst); });
    let result = run_fixed_worker(&p, parent.path(), FixedMode::Wait, Duration::from_secs(10), &cancel).unwrap();
    setter.join().unwrap();
    assert_eq!(result, FixedOutcome::Cancelled);
    assert_eq!(std::fs::read_dir(parent.path()).unwrap().count(), 0);
}
