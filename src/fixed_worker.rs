//! Fixed-code lifecycle experiment, not a launcher for untrusted code.
//! It stages one project and can run only the built-in diagnostic below.
//! It does not authorize autonomous execution or establish a security boundary.
use crate::Project;
use std::{path::Path, sync::atomic::AtomicBool, time::Duration};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixedMode { Inspect, Wait, Flood, Fail }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixedOutcome { Completed, Cancelled, TimedOut, OutputLimit, Failed }

/// Run only a built-in diagnostic, never caller/model-provided code. The parent
/// and installed system tools must be trusted. Timeouts/cancellation are best
/// effort, not a hard OS resource boundary; the output is never returned.
#[cfg(target_os = "linux")]
pub fn run_fixed_worker(project: &Project, trusted_parent: &Path, mode: FixedMode,
                        timeout: Duration, cancelled: &AtomicBool) -> Result<FixedOutcome, String> {
    use std::{fs, io::{Read, Seek, SeekFrom}, os::unix::{fs::PermissionsExt, process::CommandExt}, process::{Command, Stdio}, sync::atomic::Ordering, thread, time::Instant};
    const OUTPUT_LIMIT: u64 = 4096;
    const FILE_LIMIT: libc::rlim_t = 8192;
    const WORKER: &str = r#"import json, os, sys, time
mode = sys.argv[1]
if mode == 'wait':
    time.sleep(60)
elif mode == 'flood':
    sys.stdout.write('x' * 100000)
    sys.stdout.flush()
elif mode == 'fail':
    sys.exit(7)
else:
    # Ignore the directory descriptor that disappears when listdir closes it.
    # Do not print descriptor targets or other inherited data.
    for entry in os.listdir('/proc/self/fd'):
        if int(entry) > 2:
            try:
                os.readlink('/proc/self/fd/' + entry)
            except FileNotFoundError:
                continue
            raise RuntimeError('unexpected inherited descriptor')
    with open('/work/project.perveance.json', 'r', encoding='utf-8') as f:
        p = json.load(f)
    print(json.dumps({'width': p['width'], 'height': p['height'], 'layers': len(p['layers'])}))
"#;
    if timeout.is_zero() || timeout > Duration::from_secs(30) { return Err("fixed worker timeout must be greater than zero and at most 30 seconds".into()); }
    if cancelled.load(Ordering::SeqCst) { return Ok(FixedOutcome::Cancelled); }
    // These checks are not race-resistant and trust the system installation.
    let helper = fs::symlink_metadata("/usr/bin/bwrap").map_err(|_| "bubblewrap unavailable")?;
    if !helper.file_type().is_file() || helper.permissions().mode() & 0o6000 != 0 {
        return Err("bubblewrap must be a non-setuid regular file".into());
    }
    // Reject file capabilities as well as setuid/setgid. Treat unexpected xattr
    // failures as unavailable, never as permission to continue.
    let path = b"/usr/bin/bwrap\0";
    let attr = b"security.capability\0";
    let n = unsafe { libc::getxattr(path.as_ptr().cast(), attr.as_ptr().cast(), std::ptr::null_mut(), 0) };
    if n >= 0 { return Err("bubblewrap file capabilities are not permitted".into()); }
    let e = std::io::Error::last_os_error().raw_os_error();
    if e != Some(libc::ENODATA) { return Err("cannot verify bubblewrap file capabilities".into()); }
    struct ChildGuard { child: std::process::Child, reaped: bool }
    impl Drop for ChildGuard {
        fn drop(&mut self) {
            if !self.reaped {
                let _ = self.child.kill();
                let _ = self.child.wait();
            }
        }
    }
    let staged = crate::workspace::stage_project(project, trusted_parent)?;
    let stage_dir = staged.project_path().parent().ok_or("missing staging directory")?.to_owned();
    let work = (|| -> Result<FixedOutcome, String> {
        let mut stdout = tempfile::tempfile_in(trusted_parent).map_err(|_| "cannot create diagnostic output")?;
        let stderr = tempfile::tempfile_in(trusted_parent).map_err(|_| "cannot create diagnostic output")?;
        let mut command = Command::new("/usr/bin/bwrap");
        command.args(["--unshare-user", "--unshare-pid", "--unshare-net", "--unshare-ipc", "--unshare-uts", "--new-session", "--die-with-parent", "--clearenv", "--setenv", "PATH", "/usr/bin:/bin"]);
        for path in ["/usr", "/bin", "/lib", "/lib64"] {
            if Path::new(path).exists() { command.args(["--ro-bind", path, path]); }
        }
        let arg = match mode { FixedMode::Inspect => "inspect", FixedMode::Wait => "wait", FixedMode::Flood => "flood", FixedMode::Fail => "fail" };
        command.args(["--proc", "/proc", "--dev", "/dev", "--tmpfs", "/tmp", "--bind"])
            .arg(&stage_dir).args(["/work", "--chdir", "/work", "/usr/bin/python3", "-I", "-c", WORKER, arg])
            .env_clear().env("PATH", "/usr/bin:/bin")
            .stdin(Stdio::null()).stdout(Stdio::from(stdout.try_clone().map_err(|_| "cannot duplicate diagnostic output")?))
            .stderr(Stdio::from(stderr.try_clone().map_err(|_| "cannot duplicate diagnostic output")?));
        // Only an async-signal-safe syscall is performed after fork. This cap
        // bounds each child's regular-file output, not CPU, memory or total disk.
        unsafe { command.pre_exec(|| {
            let limit = libc::rlimit { rlim_cur: FILE_LIMIT, rlim_max: FILE_LIMIT };
            if libc::setrlimit(libc::RLIMIT_FSIZE, &limit) != 0 { return Err(std::io::Error::last_os_error()); }
            Ok(())
        }); }
        if cancelled.load(Ordering::SeqCst) { return Ok(FixedOutcome::Cancelled); }
        let mut guard = ChildGuard { child: command.spawn().map_err(|_| "cannot start fixed sandbox diagnostic")?, reaped: false };
        let started = Instant::now();
        let (status, forced) = loop {
            let stop = if cancelled.load(Ordering::SeqCst) { Some(FixedOutcome::Cancelled) }
                else if started.elapsed() >= timeout { Some(FixedOutcome::TimedOut) }
                else if stdout.metadata().map_err(|_| "cannot inspect diagnostic output")?.len() > OUTPUT_LIMIT
                    || stderr.metadata().map_err(|_| "cannot inspect diagnostic output")?.len() > OUTPUT_LIMIT { Some(FixedOutcome::OutputLimit) }
                else { None };
            if let Some(reason) = stop {
                // This signals only the bubblewrap process. Its PID namespace and
                // die-with-parent are not a proven replacement for cgroup cleanup.
                guard.child.kill().map_err(|_| "cannot stop fixed diagnostic")?;
                let status = guard.child.wait().map_err(|_| "cannot reap fixed diagnostic")?;
                guard.reaped = true;
                break (status, Some(reason));
            }
            if let Some(status) = guard.child.try_wait().map_err(|_| "cannot wait for fixed diagnostic")? {
                guard.reaped = true;
                break (status, None);
            }
            thread::sleep(Duration::from_millis(10));
        };
        if let Some(reason) = forced { return Ok(reason); }
        if stdout.metadata().map_err(|_| "cannot inspect diagnostic output")?.len() > OUTPUT_LIMIT
            || stderr.metadata().map_err(|_| "cannot inspect diagnostic output")?.len() > OUTPUT_LIMIT { return Ok(FixedOutcome::OutputLimit); }
        if !status.success() { return Ok(FixedOutcome::Failed); }
        if mode != FixedMode::Inspect { return Ok(FixedOutcome::Failed); }
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Summary { width: u32, height: u32, layers: usize }
        stdout.seek(SeekFrom::Start(0)).map_err(|_| "cannot read diagnostic output")?;
        let mut bytes = Vec::new();
        stdout.take(OUTPUT_LIMIT + 1).read_to_end(&mut bytes).map_err(|_| "cannot read diagnostic output")?;
        if bytes.len() as u64 > OUTPUT_LIMIT { return Ok(FixedOutcome::OutputLimit); }
        let value: Summary = serde_json::from_slice(&bytes).map_err(|_| "invalid fixed diagnostic output")?;
        if (value.width, value.height, value.layers) != (project.width, project.height, project.layers.len()) {
            return Err("fixed diagnostic project mismatch".into());
        }
        Ok(FixedOutcome::Completed)
    })();
    staged.close()?;
    work
}

#[cfg(not(target_os = "linux"))]
pub fn run_fixed_worker(_project: &Project, _trusted_parent: &Path, _mode: FixedMode,
                        _timeout: Duration, _cancelled: &AtomicBool) -> Result<FixedOutcome, String> {
    Err("fixed sandbox diagnostic requires Linux".into())
}
