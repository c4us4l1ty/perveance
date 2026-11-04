//! Opt-in, bounded-per-session project checkpoints. Not a crash-proof journal.
use crate::Project;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const RECOVERY_INTERVAL: Duration = Duration::from_secs(15);
pub const RECOVERY_LIMIT: usize = 8;
static SESSION_NUMBER: AtomicU64 = AtomicU64::new(0);

pub struct RecoverySession {
    directory: PathBuf,
    prefix: String,
    next_attempt: Instant,
    last_snapshot: Option<Project>,
    written: usize,
}

impl RecoverySession {
    /// The caller must choose a trusted, existing directory. Symlinks are followed.
    pub fn new(directory: &Path, now: Instant) -> Result<Self, String> {
        let metadata = std::fs::metadata(directory).map_err(|e| format!("Recovery directory: {e}"))?;
        if !metadata.is_dir() { return Err("Recovery path must be a directory.".into()); }
        let seconds = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
        let serial = SESSION_NUMBER.fetch_add(1, Ordering::Relaxed);
        let prefix = format!("perveance-recovery-{}-{seconds}-{serial}", std::process::id());
        Ok(Self { directory: directory.to_path_buf(), prefix, next_attempt: now + RECOVERY_INTERVAL, last_snapshot: None, written: 0 })
    }

    /// A new document must not inherit the previous document's deduplication state.
    pub fn forget_last_snapshot(&mut self) { self.last_snapshot = None; }
    pub fn is_full(&self) -> bool { self.written >= RECOVERY_LIMIT }
    pub fn delay(&self, now: Instant) -> Duration { self.next_attempt.saturating_duration_since(now) }

    /// Save a new snapshot only after the interval, when dirty and not mid-gesture.
    /// No existing file is replaced. A failure is returned to disable the session.
    pub fn checkpoint_if_due(&mut self, project: &Project, dirty: bool, eligible: bool, now: Instant) -> Result<Option<PathBuf>, String> {
        if now < self.next_attempt { return Ok(None); }
        if !eligible {
            self.next_attempt = now + Duration::from_secs(1);
            return Ok(None);
        }
        self.next_attempt = now + RECOVERY_INTERVAL;
        if !dirty || self.last_snapshot.as_ref() == Some(project) { return Ok(None); }
        if self.is_full() { return Err("Recovery checkpoint limit reached.".into()); }
        let path = self.directory.join(format!("{}-{:02}.perveance.json", self.prefix, self.written + 1));
        project.save_new(&path).map_err(|e| format!("Recovery checkpoint failed: {e}"))?;
        self.last_snapshot = Some(project.clone());
        self.written += 1;
        Ok(Some(path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checkpoints_are_timed_no_clobber_bounded_and_reopenable() {
        let dir = tempfile::tempdir().unwrap();
        let now = Instant::now();
        let mut s = RecoverySession::new(dir.path(), now).unwrap();
        let mut p = Project::new(2, 2).unwrap();
        p.set_pixel(0, 0, 0, [1, 2, 3, 4]).unwrap();
        assert!(s.checkpoint_if_due(&p, true, true, now).unwrap().is_none());
        assert!(s.checkpoint_if_due(&p, true, false, now + RECOVERY_INTERVAL).unwrap().is_none());
        let first_time = now + RECOVERY_INTERVAL + Duration::from_secs(1);
        let first = s.checkpoint_if_due(&p, true, true, first_time).unwrap().unwrap();
        assert_eq!(Project::load(&first).unwrap(), p);
        assert!(s.checkpoint_if_due(&p, true, true, first_time + RECOVERY_INTERVAL).unwrap().is_none());
        assert!(s.checkpoint_if_due(&p, false, true, first_time + RECOVERY_INTERVAL * 2).unwrap().is_none());
        let collision = dir.path().join(format!("{}-02.perveance.json", s.prefix));
        std::fs::write(&collision, b"preserve").unwrap();
        p.set_pixel(0, 1, 1, [4, 3, 2, 1]).unwrap();
        assert!(s.checkpoint_if_due(&p, true, true, first_time + RECOVERY_INTERVAL * 3).is_err());
        assert_eq!(std::fs::read(&collision).unwrap(), b"preserve");
        std::fs::remove_file(collision).unwrap();
        for i in 1..RECOVERY_LIMIT {
            p.set_pixel(0, 1, 1, [i as u8, 2, 3, 4]).unwrap();
            let path = s.checkpoint_if_due(&p, true, true, first_time + RECOVERY_INTERVAL * (3 + i as u32)).unwrap().unwrap();
            assert_eq!(Project::load(&path).unwrap(), p);
        }
        assert!(s.is_full());
        p.set_pixel(0, 1, 1, [99, 2, 3, 4]).unwrap();
        assert!(s.checkpoint_if_due(&p, true, true, first_time + RECOVERY_INTERVAL * 20).is_err());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), RECOVERY_LIMIT);
    }

    #[test]
    fn invalid_directories_fail_before_writing() {
        let dir = tempfile::tempdir().unwrap();
        assert!(RecoverySession::new(&dir.path().join("missing"), Instant::now()).is_err());
        let file = dir.path().join("file");
        std::fs::write(&file, b"keep").unwrap();
        assert!(RecoverySession::new(&file, Instant::now()).is_err());
        assert_eq!(std::fs::read(file).unwrap(), b"keep");
    }
}
