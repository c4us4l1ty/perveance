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
    let no = AtomicBool::new(false);
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
