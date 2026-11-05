use super::{Limits, Mode, Outcome, RecoveryReport, Report, Usage};
use crate::{Project, worker_io::{PrivateDirectory, ProjectSnapshot, anonymous_file}};
use std::{
    collections::BTreeMap,
    ffi::CString,
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    os::{fd::{AsRawFd, FromRawFd, RawFd}, unix::{fs::{MetadataExt, OpenOptionsExt, PermissionsExt}, process::CommandExt}},
    path::{Component, Path, PathBuf},
    process::{Child, ChildStdin, ChildStderr, Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};

pub(super) mod brokered;

const PREFIX: &str = "perveance-worker-";
const CONTROL_LIMIT: u64 = 4096;
const OUTPUT_LIMIT: u64 = 4096;
const FILE_LIMIT: libc::rlim_t = 8192;
const CGROUP2_MAGIC: libc::c_long = 0x6367_7270;
const DIAGNOSTIC: &str = include_str!("diagnostic.py");
const WATCHDOG: &str = include_str!("watchdog.py");

fn fail(context: &str) -> String { format!("bounded worker: {context}") }

fn open_at(directory: &File, name: &str, flags: libc::c_int) -> Result<File, String> {
    let name = CString::new(name).map_err(|_| fail("invalid control name"))?;
    let fd = unsafe { libc::openat(directory.as_raw_fd(), name.as_ptr(), flags | libc::O_CLOEXEC | libc::O_NOFOLLOW) };
    if fd < 0 { return Err(fail("required cgroup control unavailable or denied")); }
    Ok(unsafe { File::from_raw_fd(fd) })
}

fn read_control(directory: &File, name: &str) -> Result<String, String> {
    let file = open_at(directory, name, libc::O_RDONLY)?;
    let mut data = Vec::new();
    file.take(CONTROL_LIMIT + 1).read_to_end(&mut data).map_err(|_| fail("cannot read cgroup control"))?;
    if data.len() as u64 > CONTROL_LIMIT { return Err(fail("oversized cgroup control")); }
    String::from_utf8(data).map_err(|_| fail("invalid cgroup control"))
}

fn write_control(directory: &File, name: &str, value: &str) -> Result<(), String> {
    let mut file = open_at(directory, name, libc::O_WRONLY)?;
    file.write_all(value.as_bytes()).map_err(|_| fail("cannot enforce required cgroup control"))
}

fn set_control(directory: &File, name: &str, value: &str) -> Result<(), String> {
    write_control(directory, name, value)?;
    if read_control(directory, name)?.trim() != value { return Err(fail("cgroup limit readback mismatch")); }
    Ok(())
}

fn counters(value: &str) -> Result<BTreeMap<&str, u64>, String> {
    let mut result = BTreeMap::new();
    for line in value.lines() {
        let fields: Vec<_> = line.split_ascii_whitespace().collect();
        if fields.len() != 2 || !fields[0].is_ascii() || fields[1].is_empty()
            || !fields[1].bytes().all(|b| b.is_ascii_digit()) {
            return Err(fail("invalid cgroup counter"));
        }
        let number = fields[1].parse().map_err(|_| fail("cgroup counter overflow"))?;
        if result.insert(fields[0], number).is_some() { return Err(fail("duplicate cgroup counter")); }
    }
    Ok(result)
}

fn counter(directory: &File, name: &str, key: &str) -> Result<u64, String> {
    counters(&read_control(directory, name)?)?.get(key).copied().ok_or_else(|| fail("required cgroup counter missing"))
}

fn populated(directory: &File) -> Result<bool, String> {
    match counter(directory, "cgroup.events", "populated")? {
        0 => Ok(false), 1 => Ok(true), _ => Err(fail("invalid cgroup populated state")),
    }
}

fn check_cgroup(directory: &File) -> Result<(), String> {
    let mut stat: libc::statfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::fstatfs(directory.as_raw_fd(), &mut stat) } != 0 || stat.f_type != CGROUP2_MAGIC {
        return Err(fail("a real delegated cgroup-v2 directory is required; ordinary files cannot attest limits"));
    }
    let metadata = directory.metadata().map_err(|_| fail("cannot inspect cgroup directory"))?;
    if metadata.uid() != unsafe { libc::geteuid() } || metadata.permissions().mode() & 0o077 != 0 {
        return Err(fail("cgroup directory must be owned by this user and mode 0700"));
    }
    if read_control(directory, "cgroup.type")?.trim() != "domain" { return Err(fail("a domain cgroup is required")); }
    Ok(())
}

fn managed_name(value: &str) -> bool {
    value.strip_prefix(PREFIX).is_some_and(|suffix| suffix.len() == 32
        && suffix.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
}

struct Root { directory: File, path: PathBuf }
impl Root {
    fn open(path: &Path) -> Result<Self, String> {
        if unsafe { libc::geteuid() } == 0 { return Err(fail("run diagnostics as an unprivileged user, never root")); }
        if !path.is_absolute() || path.components().any(|c| !matches!(c, Component::RootDir | Component::Normal(_))) {
            return Err(fail("delegated root must be an absolute normal path"));
        }
        // Ancestors are trusted. All subsequent control opens, mkdir and rmdir
        // are relative to held directory descriptors with no final symlinks.
        let directory = OpenOptions::new().read(true).custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(path).map_err(|_| fail("delegated cgroup root unavailable"))?;
        check_cgroup(&directory)?;
        if unsafe { libc::flock(directory.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err(fail("delegated root busy; recovery never interrupts a live cooperating supervisor"));
        }
        if !read_control(&directory, "cgroup.procs")?.trim().is_empty() {
            return Err(fail("delegated root must contain no processes; place the supervisor in a sibling leaf"));
        }
        let enabled = read_control(&directory, "cgroup.subtree_control")?;
        for required in ["cpu", "memory", "pids"] {
            if !enabled.split_ascii_whitespace().any(|value| value == required) {
                return Err(fail("cpu, memory and pids controllers must already be delegated and enabled"));
            }
        }
        Ok(Self { directory, path: path.to_owned() })
    }

    fn managed(&self) -> Result<Vec<String>, String> {
        let mut names = Vec::new();
        // The root/ancestors are trusted and held locked by cooperating callers.
        // Never recurse into foreign directories or consume arbitrary filenames.
        for (index, entry) in fs::read_dir(&self.path).map_err(|_| fail("cannot inspect delegated root"))?.enumerate() {
            if index >= 256 { return Err(fail("delegated root entry limit exceeded")); }
            let entry = entry.map_err(|_| fail("cannot inspect delegated root entry"))?;
            if let Some(name) = entry.file_name().to_str().filter(|name| managed_name(name)) {
                names.push(name.to_owned());
                if names.len() > 32 { return Err(fail("too many abandoned worker cgroups; operator cleanup required")); }
            }
        }
        names.sort();
        Ok(names)
    }
}

struct Group<'a> { root: &'a Root, directory: File, name: String, removed: bool }
impl<'a> Group<'a> {
    fn create(root: &'a Root, limits: &Limits) -> Result<Self, String> {
        let mut bytes = [0u8; 16];
        if unsafe { libc::getrandom(bytes.as_mut_ptr().cast(), bytes.len(), 0) } != bytes.len() as isize {
            return Err(fail("cannot allocate unpredictable cgroup name"));
        }
        let name = format!("{PREFIX}{}", bytes.iter().map(|b| format!("{b:02x}")).collect::<String>());
        let c_name = CString::new(name.as_bytes()).map_err(|_| fail("invalid managed cgroup name"))?;
        if unsafe { libc::mkdirat(root.directory.as_raw_fd(), c_name.as_ptr(), 0o700) } != 0 {
            return Err(fail("cannot create delegated worker cgroup"));
        }
        let directory = match open_at(&root.directory, &name, libc::O_RDONLY | libc::O_DIRECTORY) {
            Ok(directory) => directory,
            Err(error) => {
                unsafe { libc::unlinkat(root.directory.as_raw_fd(), c_name.as_ptr(), libc::AT_REMOVEDIR); }
                return Err(error);
            }
        };
        // kernfs uses its own initial mode rather than mkdir's mode argument.
        let group = Self { root, directory, name, removed: false };
        group.directory.set_permissions(fs::Permissions::from_mode(0o700)).map_err(|_| fail("cannot make worker cgroup private"))?;
        check_cgroup(&group.directory)?;
        for (name, value) in [
            ("memory.max", (limits.memory_mib * 1024 * 1024).to_string()),
            ("memory.swap.max", "0".to_owned()),
            ("memory.oom.group", "1".to_owned()),
            ("pids.max", limits.processes.to_string()),
            ("cpu.max", format!("{} 100000", limits.cpu_percent * 1000)),
            ("cgroup.max.depth", "0".to_owned()),
            ("cgroup.max.descendants", "0".to_owned()),
        ] { set_control(&group.directory, name, &value)?; }
        // Required features must exist BEFORE any worker can be launched.
        open_at(&group.directory, "cgroup.kill", libc::O_WRONLY)?;
        group.usage()?;
        if populated(&group.directory)? { return Err(fail("new cgroup unexpectedly populated")); }
        Ok(group)
    }

    fn existing(root: &'a Root, name: String) -> Result<Self, String> {
        if !managed_name(&name) { return Err(fail("invalid managed cgroup name")); }
        let directory = open_at(&root.directory, &name, libc::O_RDONLY | libc::O_DIRECTORY)?;
        if directory.metadata().map_err(|_| fail("cannot inspect abandoned group"))?.uid() != unsafe { libc::geteuid() } {
            return Err(fail("abandoned group is not owned by this user"));
        }
        // A supervisor may have died between mkdir and chmod. The private root
        // and exact random-name grammar scope recovery; never follow links.
        directory.set_permissions(fs::Permissions::from_mode(0o700)).map_err(|_| fail("cannot protect abandoned group"))?;
        check_cgroup(&directory)?;
        Ok(Self { root, directory, name, removed: false })
    }

    fn kill(&self) -> Result<(), String> { write_control(&self.directory, "cgroup.kill", "1") }

    fn usage(&self) -> Result<Usage, String> {
        let cpu = read_control(&self.directory, "cpu.stat")?;
        let fields = counters(&cpu)?;
        let cpu_value = |key| fields.get(key).copied().ok_or_else(|| fail("required CPU counter missing"));
        Ok(Usage {
            cpu_microseconds: cpu_value("usage_usec")?,
            cpu_throttled_periods: cpu_value("nr_throttled")?,
            cpu_throttled_microseconds: cpu_value("throttled_usec")?,
            memory_peak_bytes: read_control(&self.directory, "memory.peak")?.trim().parse().map_err(|_| fail("invalid memory peak counter"))?,
            oom_kills: counter(&self.directory, "memory.events", "oom_kill")?,
            process_limit_hits: counter(&self.directory, "pids.events", "max")?,
        })
    }

    fn empty(&self) -> Result<(), String> {
        self.kill()?;
        let started = Instant::now();
        while populated(&self.directory)? {
            if started.elapsed() >= Duration::from_secs(3) {
                return Err(fail("worker cgroup still populated; recovery/operator cleanup required"));
            }
            self.kill()?;
            thread::sleep(Duration::from_millis(10));
        }
        Ok(())
    }

    fn remove(&mut self) -> Result<(), String> {
        let name = CString::new(self.name.as_bytes()).map_err(|_| fail("invalid managed cgroup name"))?;
        if unsafe { libc::unlinkat(self.root.directory.as_raw_fd(), name.as_ptr(), libc::AT_REMOVEDIR) } != 0 {
            return Err(fail("cannot remove worker cgroup; explicit recovery required"));
        }
        self.removed = true;
        Ok(())
    }

    fn finish(mut self) -> Result<Usage, String> {
        self.empty()?;
        let usage = self.usage()?;
        self.remove()?;
        Ok(usage)
    }
}
impl Drop for Group<'_> {
    fn drop(&mut self) {
        if !self.removed {
            // Last-resort, non-waiting cleanup. An unsuccessful cleanup is never
            // reported as success; keep the bounded group for explicit recovery.
            let _ = self.kill();
            if populated(&self.directory) == Ok(false) { let _ = self.remove(); }
        }
    }
}

/// Only async-signal-safe syscalls in pre_exec. In particular, no formatting,
/// locks, allocation, Python, filesystem traversal or Rust unwinding after fork.
fn hygiene(command: &mut Command, join: Option<RawFd>, preserve: Option<RawFd>) {
    let parent = unsafe { libc::getpid() };
    unsafe { command.pre_exec(move || {
        if join.is_some() {
            if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL, 0, 0, 0) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            if libc::getppid() != parent { return Err(std::io::Error::from_raw_os_error(libc::ESRCH)); }
        }
        const CLOSE_RANGE_CLOEXEC: libc::c_uint = 1 << 2;
        if libc::syscall(libc::SYS_close_range, 3u32, u32::MAX, CLOSE_RANGE_CLOEXEC) != 0
            || libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0 {
            return Err(std::io::Error::last_os_error());
        }
        if let Some(fd) = preserve {
            if libc::fcntl(fd, libc::F_SETFD, 0) != 0 { return Err(std::io::Error::last_os_error()); }
        }
        let file_limit = libc::rlimit { rlim_cur: FILE_LIMIT, rlim_max: FILE_LIMIT };
        let fd_limit = libc::rlimit { rlim_cur: 64, rlim_max: 64 };
        if libc::setrlimit(libc::RLIMIT_FSIZE, &file_limit) != 0 || libc::setrlimit(libc::RLIMIT_NOFILE, &fd_limit) != 0 {
            return Err(std::io::Error::last_os_error());
        }
        if let Some(fd) = join {
            // '0' moves the writing task itself, before exec or any helper fork.
            if libc::write(fd, b"0".as_ptr().cast(), 1) != 1 { return Err(std::io::Error::last_os_error()); }
        }
        Ok(())
    }); }
}

fn verify_tools() -> Result<(), String> {
    for path in ["/usr/bin/bwrap", "/usr/bin/python3"] {
        let metadata = if path.ends_with("bwrap") { fs::symlink_metadata(path) } else { fs::metadata(path) }
            .map_err(|_| fail("trusted bubblewrap and system Python are required"))?;
        if !metadata.file_type().is_file() || metadata.uid() != 0 || metadata.permissions().mode() & 0o6022 != 0 {
            return Err(fail("system tools must be root-owned, non-privileged and not group/world writable"));
        }
    }
    let path = b"/usr/bin/bwrap\0";
    let attribute = b"security.capability\0";
    let value = unsafe { libc::getxattr(path.as_ptr().cast(), attribute.as_ptr().cast(), std::ptr::null_mut(), 0) };
    if value >= 0 || std::io::Error::last_os_error().raw_os_error() != Some(libc::ENODATA) {
        return Err(fail("cannot establish that bubblewrap has no file capabilities"));
    }
    Ok(())
}

fn ready(fd: RawFd, timeout_ms: libc::c_int) -> Result<bool, String> {
    let mut poll = libc::pollfd { fd, events: libc::POLLIN, revents: 0 };
    let result = unsafe { libc::poll(&mut poll, 1, timeout_ms) };
    if result < 0 { return Err(fail("watchdog protocol poll failed")); }
    Ok(result > 0)
}

struct Watchdog { child: Child, lease: Option<ChildStdin>, protocol: ChildStderr, finished: bool }
impl Watchdog {
    fn start(group: &Group<'_>, limits: &Limits) -> Result<Self, String> {
        let directory = group.directory.try_clone().map_err(|_| fail("cannot duplicate watchdog directory"))?;
        let mut command = Command::new("/usr/bin/python3");
        command.args(["-I", "-c", WATCHDOG, &limits.wall_seconds.to_string(), &(limits.cpu_seconds * 1_000_000).to_string(), &directory.as_raw_fd().to_string()])
            .env_clear().stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::piped());
        hygiene(&mut command, None, Some(directory.as_raw_fd()));
        let mut child = command.spawn().map_err(|_| fail("cannot start independent watchdog"))?;
        let lease = child.stdin.take();
        let protocol = child.stderr.take().ok_or_else(|| fail("missing watchdog protocol"))?;
        let mut guard = Self { child, lease, protocol, finished: false };
        if !ready(guard.protocol.as_raw_fd(), 2000)? { return Err(fail("independent watchdog did not become ready")); }
        let mut byte = [0u8; 1];
        guard.protocol.read_exact(&mut byte).map_err(|_| fail("independent watchdog startup failed"))?;
        if byte != *b"R" { return Err(fail("invalid watchdog startup protocol")); }
        Ok(guard)
    }

    fn reason(&mut self) -> Result<Option<Outcome>, String> {
        if self.child.try_wait().map_err(|_| fail("cannot inspect watchdog"))?.is_some() {
            return Err(fail("independent watchdog exited unexpectedly; worker stopped"));
        }
        if !ready(self.protocol.as_raw_fd(), 0)? { return Ok(None); }
        self.read_reason()
    }

    fn read_reason(&mut self) -> Result<Option<Outcome>, String> {
        let mut byte = [0u8; 1];
        match self.protocol.read(&mut byte).map_err(|_| fail("cannot read watchdog protocol"))? {
            0 => Ok(None),
            1 => match byte[0] {
                b'C' => Ok(Some(Outcome::CpuBudget)), b'T' => Ok(Some(Outcome::TimedOut)),
                _ => Err(fail("invalid watchdog stop protocol")),
            },
            _ => Err(fail("invalid watchdog protocol size")),
        }
    }

    fn finish(mut self) -> Result<Option<Outcome>, String> {
        self.lease.take(); // EOF revokes the lease, including all descendants.
        let started = Instant::now();
        loop {
            if let Some(status) = self.child.try_wait().map_err(|_| fail("cannot reap watchdog"))? {
                self.finished = true;
                if !status.success() { return Err(fail("independent watchdog cleanup failed; recovery required")); }
                return self.read_reason();
            }
            if started.elapsed() >= Duration::from_secs(4) { return Err(fail("independent watchdog cleanup timed out")); }
            thread::sleep(Duration::from_millis(10));
        }
    }
}
impl Drop for Watchdog {
    fn drop(&mut self) {
        if !self.finished {
            self.lease.take();
            let started = Instant::now();
            while started.elapsed() < Duration::from_secs(4) {
                if let Ok(Some(_)) = self.child.try_wait() { self.finished = true; return; }
                thread::sleep(Duration::from_millis(10));
            }
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

struct WorkerChild<'a, 'b> { child: Child, group: &'a Group<'b>, reaped: bool }
impl Drop for WorkerChild<'_, '_> {
    fn drop(&mut self) {
        if !self.reaped {
            let _ = self.group.kill();
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

fn command(snapshot: &ProjectSnapshot, limits: &Limits, mode: Mode, stdout: &File, stderr: &File,
           broker: Option<&mut brokered::Channel<'_>>) -> Result<Command, String> {
    let mut command = Command::new("/usr/bin/bwrap");
    command.args(["--unshare-user", "--unshare-pid", "--unshare-net", "--unshare-ipc", "--unshare-uts", "--unshare-cgroup",
        "--disable-userns", "--new-session", "--die-with-parent", "--cap-drop", "ALL", "--clearenv", "--setenv", "PATH", "/usr/bin:/bin"]);
    for path in ["/usr", "/bin", "/lib", "/lib64"] {
        if Path::new(path).exists() { command.args(["--ro-bind", path, path]); }
    }
    // One size-limited writable filesystem, shared by all three scratch paths.
    // Do not use --dev: its writable tmpfs/pty hierarchy would add more storage.
    command.args(["--proc", "/proc", "--dir", "/dev"]);
    for name in ["null", "zero", "full", "random", "urandom"] {
        let path = format!("/dev/{name}");
        command.args(["--dev-bind", &path, &path]);
    }
    command.args(["--symlink", "/proc/self/fd", "/dev/fd", "--size", &(limits.scratch_mib * 1024 * 1024).to_string(), "--tmpfs", "/work",
        "--symlink", "/work", "/tmp", "--symlink", "/work", "/dev/shm", "--dir", "/input",
        "--remount-ro", "/", "--chdir", "/work", "/usr/bin/python3", "-I", "-c"])
        .env_clear().stdin(Stdio::from(snapshot.stdin_file()?))
        .stdout(Stdio::from(stdout.try_clone().map_err(|_| fail("cannot duplicate diagnostic output"))?))
        .stderr(Stdio::from(stderr.try_clone().map_err(|_| fail("cannot duplicate diagnostic output"))?));
    if let Some(channel) = broker { channel.configure(&mut command)?; }
    else { command.args([DIAGNOSTIC, mode.argument()]); }
    Ok(command)
}

fn output_large(stdout: &File, stderr: &File) -> Result<bool, String> {
    Ok(stdout.metadata().map_err(|_| fail("cannot inspect diagnostic output"))?.len() > OUTPUT_LIMIT
        || stderr.metadata().map_err(|_| fail("cannot inspect diagnostic output"))?.len() > OUTPUT_LIMIT)
}

fn verify_output(stdout: &mut File, project: &Project, mode: Mode, limits: &Limits) -> Result<Outcome, String> {
    stdout.seek(SeekFrom::Start(0)).map_err(|_| fail("cannot read diagnostic output"))?;
    let mut data = Vec::new();
    stdout.take(OUTPUT_LIMIT + 1).read_to_end(&mut data).map_err(|_| fail("cannot read diagnostic output"))?;
    if data.len() as u64 > OUTPUT_LIMIT { return Ok(Outcome::OutputLimit); }
    match mode {
        Mode::Inspect => {
            #[derive(serde::Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Summary { width: u32, height: u32, layers: usize }
            let summary: Summary = serde_json::from_slice(&data).map_err(|_| fail("invalid fixed diagnostic output"))?;
            if (summary.width, summary.height, summary.layers) != (project.width, project.height, project.layers.len()) {
                return Err(fail("fixed diagnostic project mismatch"));
            }
            Ok(Outcome::Completed)
        }
        Mode::ScratchFlood => {
            #[derive(serde::Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Full { scratch_full: bool, bytes: u64 }
            let full: Full = serde_json::from_slice(&data).map_err(|_| fail("invalid scratch diagnostic output"))?;
            if !full.scratch_full || full.bytes == 0 || full.bytes > limits.scratch_mib * 1024 * 1024 {
                return Err(fail("scratch diagnostic did not establish its limit"));
            }
            Ok(Outcome::ScratchLimit)
        }
        _ => Ok(Outcome::Failed),
    }
}

pub(super) fn run(project: &Project, trusted_parent: &Path, delegated_root: &Path, mode: Mode,
                   limits: &Limits, cancelled: &AtomicBool) -> Result<Report, String> {
    limits.validate()?;
    if cancelled.load(Ordering::SeqCst) { return Ok(Report::cancelled(limits)); }
    // Compatibility wrapper only: keep validating the legacy parent argument,
    // but never create artwork/log files there or mount it into a worker.
    PrivateDirectory::open(trusted_parent)?;
    let snapshot = ProjectSnapshot::from_project(project)?;
    run_snapshot(&snapshot, delegated_root, mode, limits, cancelled)
}

pub(super) fn run_snapshot(snapshot: &ProjectSnapshot, delegated_root: &Path, mode: Mode,
                          limits: &Limits, cancelled: &AtomicBool) -> Result<Report, String> {
    run_inner(snapshot, delegated_root, mode, limits, cancelled, None)
}

fn run_inner(snapshot: &ProjectSnapshot, delegated_root: &Path, mode: Mode,
             limits: &Limits, cancelled: &AtomicBool, mut broker: Option<brokered::Channel<'_>>) -> Result<Report, String> {
    limits.validate()?;
    if cancelled.load(Ordering::SeqCst) { return Ok(Report::cancelled(limits)); }
    let project = snapshot.project();
    let root = Root::open(delegated_root)?;
    if !root.managed()?.is_empty() { return Err(fail("abandoned worker cgroups exist; explicitly recover before starting another")); }
    verify_tools()?;
    let group = Group::create(&root, limits)?;
    let result = (|| -> Result<Outcome, String> {
        let mut stdout = anonymous_file()?;
        let mut stderr = anonymous_file()?;
        let mut watchdog = Watchdog::start(&group, limits)?;
        let join = open_at(&group.directory, "cgroup.procs", libc::O_WRONLY)?;
        let mut command = command(snapshot, limits, mode, &stdout, &stderr, broker.as_mut())?;
        hygiene(&mut command, Some(join.as_raw_fd()), None);
        let child = command.spawn().map_err(|_| fail("cannot attach/start fixed diagnostic; no unconfined fallback"))?;
        // Command can retain its Stdio source handles. The host MUST relinquish
        // the worker socket end so peer EOF really revokes the broker lease.
        drop(command);
        if let Some(channel) = &broker { channel.ready.store(true, Ordering::SeqCst); }
        let mut guard = WorkerChild { child, group: &group, reaped: false };
        let mut forced = None;
        let status = loop {
            if forced.is_none() {
                forced = if cancelled.load(Ordering::SeqCst) { Some(Outcome::Cancelled) }
                    else if output_large(&stdout, &stderr)? { Some(Outcome::OutputLimit) }
                    else { watchdog.reason()? };
                if forced.is_none() {
                    let usage = group.usage()?;
                    if usage.oom_kills > 0 { forced = Some(Outcome::MemoryLimit); }
                    else if usage.process_limit_hits > 0 { forced = Some(Outcome::ProcessLimit); }
                }
            }
            if forced.is_some() {
                if let Some(channel) = &broker { channel.revoked.store(true, Ordering::SeqCst); }
                group.kill()?;
            }
            if let Some(status) = guard.child.try_wait().map_err(|_| fail("cannot wait for fixed diagnostic"))? {
                guard.reaped = true;
                break status;
            }
            thread::sleep(Duration::from_millis(10));
        };
        if let Some(channel) = &broker { channel.revoked.store(true, Ordering::SeqCst); }
        let final_reason = watchdog.finish()?;
        group.empty()?;
        let usage = group.usage()?;
        if let Some(outcome) = forced.or(final_reason) { return Ok(outcome); }
        if usage.oom_kills > 0 { return Ok(Outcome::MemoryLimit); }
        if usage.process_limit_hits > 0 { return Ok(Outcome::ProcessLimit); }
        if output_large(&stdout, &stderr)? { return Ok(Outcome::OutputLimit); }
        if !status.success() { return Ok(Outcome::Failed); }
        if broker.is_some() { brokered::verify(&mut stderr, project) }
        else { verify_output(&mut stdout, project, mode, limits) }
    })();
    // No filesystem staging exists. A failed cgroup cleanup must still prevent
    // a completed-work report; sealed input/log descriptors close on unwind.
    let usage = group.finish()?;
    let outcome = result?;
    Ok(Report { outcome, limits: limits.clone(), worker_started: true, cgroup_limits_applied: true,
        cgroup_removed: true, usage: Some(usage), input_snapshot_sealed: true,
        input_revision: Some(snapshot.revision().to_owned()), source_files_mounted: false, fixed_code_only: true,
        autonomous_execution: false, security_boundary_verified: false })
}

pub(super) fn recover(delegated_root: &Path) -> Result<RecoveryReport, String> {
    let root = Root::open(delegated_root)?;
    let names = root.managed()?;
    let mut removed = 0;
    for name in names {
        let mut group = Group::existing(&root, name)?;
        group.empty()?;
        group.remove()?;
        removed += 1;
    }
    Ok(RecoveryReport { removed_cgroups: removed, autonomous_execution: false, staging_files_removed: false })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn managed_names_never_accept_paths_or_foreign_groups() {
        assert!(managed_name(&format!("{PREFIX}{}", "0123456789abcdef".repeat(2))));
        for value in ["", "..", "perveance-worker-", "supervisor", "perveance-worker-../supervisor"] {
            assert!(!managed_name(value));
        }
        for suffix in ["g".repeat(32), "A".repeat(32), "0".repeat(31), "0".repeat(33), format!("{}\n", "0".repeat(32))] {
            assert!(!managed_name(&format!("{PREFIX}{suffix}")));
        }
    }

    #[test]
    fn kernel_counters_are_bounded_strict_unsigned_and_nonduplicated() {
        assert_eq!(counters("usage_usec 123\nnr_periods 4\n").unwrap()["usage_usec"], 123);
        for value in ["key", "key 1 extra", "key -1", "key +1", "key 1.0", "key 18446744073709551616", "key 1\nkey 2", "key ١"] {
            assert!(counters(value).is_err(), "{value:?}");
        }
    }
}
