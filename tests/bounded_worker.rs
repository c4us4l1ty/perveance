use perveance::bounded_worker::{Limits, Mode};

#[test]
fn diagnostic_limits_and_modes_are_closed_and_bounded() {
    Limits::default().validate().unwrap();
    for field in 0..6 {
        for value in [0, u64::MAX] {
            let mut limits = Limits::default();
            match field {
                0 => limits.wall_seconds = value, 1 => limits.cpu_seconds = value,
                2 => limits.cpu_percent = value, 3 => limits.memory_mib = value,
                4 => limits.scratch_mib = value, _ => limits.processes = value,
            }
            assert!(limits.validate().is_err());
        }
    }
    for mode in ["inspect", "wait", "fork-tree", "cpu", "memory", "processes", "scratch", "output", "fail"] {
        Mode::parse(mode).unwrap();
    }
    for mode in ["", "shell", "sh -c true", "../inspect", "inspect\n", "model-code"] {
        assert!(Mode::parse(mode).is_err());
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use perveance::{bounded_worker::{self, Outcome}, Project};
    use std::{fs, os::unix::fs::symlink, path::{Path, PathBuf}, process::{Child, Command, Stdio},
        sync::{Arc, atomic::{AtomicBool, Ordering}}, thread, time::{Duration, Instant}};

    fn cli() -> Command { Command::new(env!("CARGO_BIN_EXE_perveance-cli")) }

    #[test]
    fn cancellation_and_bad_limits_cannot_create_staging_or_groups() {
        let parent = tempfile::tempdir().unwrap();
        let missing = parent.path().join("not-a-delegation");
        let project = Project::new(1, 1).unwrap();
        let cancel = AtomicBool::new(true);
        let report = bounded_worker::run(&project, parent.path(), &missing, Mode::Inspect, &Limits::default(), &cancel).unwrap();
        assert_eq!(report.outcome, Outcome::Cancelled);
        assert!(!report.worker_started && !report.cgroup_limits_applied && !report.autonomous_execution);
        assert!(report.usage.is_none());
        let bad = Limits { memory_mib: 0, ..Limits::default() };
        assert!(bounded_worker::run(&project, parent.path(), &missing, Mode::Inspect, &bad, &cancel).is_err());
        assert_eq!(fs::read_dir(parent.path()).unwrap().count(), 0);
    }

    #[test]
    fn ordinary_files_and_symlinks_never_attest_kernel_enforcement() {
        let root = tempfile::tempdir().unwrap();
        let stage = tempfile::tempdir().unwrap();
        for (name, data) in [("cgroup.type", "domain"), ("cgroup.subtree_control", "cpu memory pids"), ("cgroup.procs", "")] {
            fs::write(root.path().join(name), data).unwrap();
        }
        let project = Project::new(1, 1).unwrap();
        let no = AtomicBool::new(false);
        assert!(bounded_worker::run(&project, stage.path(), root.path(), Mode::Inspect, &Limits::default(), &no).is_err());
        assert!(bounded_worker::recover(root.path()).is_err());
        let link = stage.path().join("link");
        symlink(root.path(), &link).unwrap();
        assert!(bounded_worker::recover(&link).is_err());
        assert_eq!(fs::read_to_string(root.path().join("cgroup.subtree_control")).unwrap(), "cpu memory pids");
        assert_eq!(fs::read_dir(stage.path()).unwrap().count(), 1);
    }

    #[test]
    fn cli_discovery_is_honest_and_unsupported_roots_fail_closed() {
        let discovery = cli().arg("capabilities").output().unwrap();
        assert!(discovery.status.success());
        let value: serde_json::Value = serde_json::from_slice(&discovery.stdout).unwrap();
        assert_eq!(value["bounded_worker_diagnostic"]["fixed_code_only"], true);
        assert_eq!(value["bounded_worker_diagnostic"]["autonomous_execution"], false);
        assert_eq!(value["bounded_worker_diagnostic"]["security_boundary_verified"], false);
        assert_eq!(value["autonomy_intake"]["budgets_enforced"], false);
        assert_eq!(value["persistent_raster_tasks"]["resource_quotas_enforced"], false);
        let root = tempfile::tempdir().unwrap();
        let input = root.path().join("input.json");
        Project::new(1, 1).unwrap().save_new(&input).unwrap();
        let output = cli().arg("verify-bounded-worker").arg(&input).arg(root.path()).arg(root.path()).output().unwrap();
        assert!(!output.status.success());
        let error: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
        assert_eq!(error["ok"], false);
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
    }

    // These ignored tests MUST run on the delegated Ubuntu CI service. A missing
    // prerequisite is an error, not a successful skip or authority to run code.
    fn delegated() -> PathBuf {
        let path = std::env::var_os("PERVEANCE_TEST_CGROUP_ROOT").expect("explicit delegated CI root required");
        PathBuf::from(path)
    }

    fn groups(root: &Path) -> Vec<PathBuf> {
        fs::read_dir(root).unwrap().map(|entry| entry.unwrap().path())
            .filter(|path| path.file_name().unwrap().to_string_lossy().starts_with("perveance-worker-")).collect()
    }

    fn wait_for<T>(timeout: u64, mut check: impl FnMut() -> Option<T>) -> T {
        let started = Instant::now();
        loop {
            if let Some(value) = check() { return value; }
            assert!(started.elapsed() < Duration::from_secs(timeout), "bounded kernel test timed out");
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn live_tree(root: &Path) -> PathBuf {
        wait_for(5, || {
            for group in groups(root) {
                let processes = fs::read_to_string(group.join("cgroup.procs")).ok()?;
                // Require a Python session leader, not merely bubblewrap's own
                // helper count. This proves the detached fork actually ran.
                for pid in processes.split_ascii_whitespace() {
                    if fs::read_to_string(format!("/proc/{pid}/comm")).ok()?.trim() != "python3" { continue; }
                    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
                    let fields: Vec<_> = stat.rsplit_once(')')?.1.split_ascii_whitespace().collect();
                    if fields.get(3) == Some(&pid) && processes.lines().count() >= 4 { return Some(group); }
                }
            }
            None
        })
    }

    fn empty(group: &Path) -> bool {
        fs::read_to_string(group.join("cgroup.events")).is_ok_and(|text| text.lines().any(|line| line == "populated 0"))
    }

    struct ChildGuard(Child);
    impl Drop for ChildGuard {
        fn drop(&mut self) { let _ = self.0.kill(); let _ = self.0.wait(); }
    }

    fn spawn_tree(parent: &Path, root: &Path) -> ChildGuard {
        let input = parent.join("input.json");
        Project::new(1, 1).unwrap().save_new(&input).unwrap();
        ChildGuard(cli().arg("verify-bounded-worker").arg(input).arg(parent).arg(root).arg("fork-tree")
            .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap())
    }

    #[test]
    #[ignore = "requires unprivileged bubblewrap and a real delegated cgroup-v2 root"]
    fn kernel_resource_limits_and_cleanup() {
        use std::{fs::File, os::fd::AsRawFd};
        let root = delegated();
        assert!(groups(&root).is_empty());
        let parent = tempfile::tempdir().unwrap();
        let mut project = Project::new(2, 3).unwrap();
        project.rename_layer(0, "private-artwork-name").unwrap();
        project.layers[0].visible = false;
        project.set_pixel(0, 0, 0, [4, 5, 6, 0]).unwrap();
        let before = project.clone();
        // The cgroup descriptor and a deliberately inheritable host file must
        // not survive exec into the fixed diagnostic.
        let file = File::open("/dev/null").unwrap();
        let fd = file.as_raw_fd();
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFD) };
        assert!(flags >= 0);
        assert_eq!(unsafe { libc::fcntl(fd, libc::F_SETFD, flags & !libc::FD_CLOEXEC) }, 0);
        let limits = Limits { wall_seconds: 8, cpu_seconds: 1, cpu_percent: 100, scratch_mib: 1, ..Limits::default() };
        let no = AtomicBool::new(false);
        for (mode, expected) in [
            (Mode::Inspect, Outcome::Completed), (Mode::Fail, Outcome::Failed),
            (Mode::OutputFlood, Outcome::OutputLimit), (Mode::ScratchFlood, Outcome::ScratchLimit),
            (Mode::MemoryFlood, Outcome::MemoryLimit), (Mode::PidFlood, Outcome::ProcessLimit),
            (Mode::CpuBurn, Outcome::CpuBudget),
        ] {
            let report = bounded_worker::run(&project, parent.path(), &root, mode, &limits, &no).unwrap();
            assert_eq!(report.outcome, expected, "mode {mode:?}");
            assert!(report.worker_started && report.cgroup_limits_applied && report.cgroup_removed);
            assert!(!report.autonomous_execution && !report.security_boundary_verified);
            let usage = report.usage.as_ref().unwrap();
            assert!(usage.memory_peak_bytes > 0 && usage.cpu_microseconds > 0);
            if mode == Mode::MemoryFlood { assert!(usage.oom_kills > 0); }
            if mode == Mode::PidFlood { assert!(usage.process_limit_hits > 0); }
            if mode == Mode::CpuBurn { assert!(usage.cpu_microseconds >= 1_000_000); }
            assert!(!serde_json::to_string(&report).unwrap().contains("private-artwork-name"));
            assert!(groups(&root).is_empty(), "cgroup leak after {mode:?}");
            assert_eq!(fs::read_dir(parent.path()).unwrap().count(), 0, "staging leak after {mode:?}");
            println!("bounded kernel diagnostic {mode:?}: {expected:?}; cgroup removed");
        }
        assert_eq!(unsafe { libc::fcntl(fd, libc::F_SETFD, flags) }, 0);
        assert_eq!(project, before);
    }

    #[test]
    #[ignore = "requires unprivileged bubblewrap and a real delegated cgroup-v2 root"]
    fn cancellation_kills_detached_descendants_and_locks_out_recovery() {
        let root = delegated();
        let parent = tempfile::tempdir().unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        struct CancelOnDrop(Arc<AtomicBool>);
        impl Drop for CancelOnDrop { fn drop(&mut self) { self.0.store(true, Ordering::SeqCst); } }
        let _on_failure = CancelOnDrop(Arc::clone(&cancel));
        let thread_cancel = Arc::clone(&cancel);
        let thread_root = root.clone();
        let stage = parent.path().to_owned();
        let worker = thread::spawn(move || bounded_worker::run(&Project::new(1, 1).unwrap(), &stage, &thread_root,
            Mode::ForkTree, &Limits::default(), &thread_cancel));
        let group = live_tree(&root);
        for (file, expected) in [("memory.max", "67108864"), ("memory.swap.max", "0"), ("memory.oom.group", "1"),
            ("pids.max", "12"), ("cpu.max", "50000 100000"), ("cgroup.max.depth", "0")] {
            assert_eq!(fs::read_to_string(group.join(file)).unwrap().trim(), expected);
        }
        assert!(bounded_worker::recover(&root).is_err(), "live supervisor lock must block recovery");
        cancel.store(true, Ordering::SeqCst);
        assert_eq!(worker.join().unwrap().unwrap().outcome, Outcome::Cancelled);
        assert!(!group.exists());
        assert!(groups(&root).is_empty());
        assert_eq!(fs::read_dir(parent.path()).unwrap().count(), 0);
    }

    #[test]
    #[ignore = "requires unprivileged bubblewrap and a real delegated cgroup-v2 root"]
    fn supervisor_sigkill_leaves_no_live_descendants_and_explicit_recovery_removes_group() {
        let root = delegated();
        let parent = tempfile::tempdir().unwrap();
        let mut supervisor = spawn_tree(parent.path(), &root);
        let group = live_tree(&root);
        supervisor.0.kill().unwrap();
        supervisor.0.wait().unwrap();
        wait_for(5, || empty(&group).then_some(()));
        assert!(group.exists(), "watchdog does not silently remove recovery evidence");
        assert_eq!(bounded_worker::recover(&root).unwrap().removed_cgroups, 1);
        assert_eq!(bounded_worker::recover(&root).unwrap().removed_cgroups, 0);
        assert!(!group.exists());
        // Original input and abandoned staging are NOT silently deleted by
        // cgroup recovery; they may contain artwork and need retention policy.
        assert!(parent.path().join("input.json").exists());
    }

    #[test]
    #[ignore = "requires unprivileged bubblewrap and a real delegated cgroup-v2 root"]
    fn wall_watchdog_survives_a_stopped_supervisor() {
        let root = delegated();
        let parent = tempfile::tempdir().unwrap();
        let mut supervisor = spawn_tree(parent.path(), &root);
        let group = live_tree(&root);
        assert_eq!(unsafe { libc::kill(supervisor.0.id() as libc::pid_t, libc::SIGSTOP) }, 0);
        wait_for(13, || empty(&group).then_some(()));
        assert!(supervisor.0.try_wait().unwrap().is_none());
        supervisor.0.kill().unwrap();
        supervisor.0.wait().unwrap();
        assert_eq!(bounded_worker::recover(&root).unwrap().removed_cgroups, 1);
    }

    #[test]
    #[ignore = "requires unprivileged bubblewrap and a real delegated cgroup-v2 root"]
    fn recovery_is_explicit_and_never_removes_foreign_cgroups() {
        let root = delegated();
        let stage = tempfile::tempdir().unwrap();
        let stale = root.join(format!("perveance-worker-{}", "0".repeat(32)));
        let foreign = root.join("foreign-do-not-touch");
        let malformed = root.join("perveance-worker-not-a-managed-id");
        for path in [&stale, &foreign, &malformed] { fs::create_dir(path).unwrap(); }
        assert!(bounded_worker::run(&Project::new(1, 1).unwrap(), stage.path(), &root,
            Mode::Inspect, &Limits::default(), &AtomicBool::new(false)).is_err());
        assert_eq!(fs::read_dir(stage.path()).unwrap().count(), 0);
        let report = bounded_worker::recover(&root).unwrap();
        assert_eq!(report.removed_cgroups, 1);
        assert!(!report.autonomous_execution && !report.staging_files_removed);
        assert!(!stale.exists() && foreign.exists() && malformed.exists());
        fs::remove_dir(&foreign).unwrap();
        fs::remove_dir(&malformed).unwrap();
    }

    #[test]
    #[ignore = "requires unprivileged bubblewrap and a real delegated cgroup-v2 root"]
    fn watchdog_loss_fails_closed_in_the_live_supervisor() {
        let root = delegated();
        let parent = tempfile::tempdir().unwrap();
        let mut supervisor = spawn_tree(parent.path(), &root);
        let _group = live_tree(&root);
        let children = fs::read_to_string(format!("/proc/{0}/task/{0}/children", supervisor.0.id())).unwrap();
        let watchdog = children.split_ascii_whitespace().find_map(|pid| {
            let cmdline = fs::read(format!("/proc/{pid}/cmdline")).ok()?;
            cmdline.windows(b"Embedded trusted watchdog".len()).any(|part| part == b"Embedded trusted watchdog")
                .then(|| pid.parse::<libc::pid_t>().unwrap())
        }).expect("independent watchdog child");
        assert_eq!(unsafe { libc::kill(watchdog, libc::SIGKILL) }, 0);
        let status = wait_for(5, || supervisor.0.try_wait().unwrap());
        assert!(!status.success());
        assert!(groups(&root).is_empty());
        assert_eq!(fs::read_dir(parent.path()).unwrap().count(), 1, "only original input remains");
    }
}
