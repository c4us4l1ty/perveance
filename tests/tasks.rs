#![cfg(target_os = "linux")]

use perveance::{tasks::{self, TaskReason, TaskState, INPUT_PROJECT, OUTPUT_PNG, OUTPUT_PROJECT}, EditProposal, Operation, Project};
use std::{fs, os::unix::fs::PermissionsExt, process::Command, sync::{Arc, Barrier}};

fn private_root() -> tempfile::TempDir {
    tempfile::Builder::new().permissions(fs::Permissions::from_mode(0o700)).tempdir().unwrap()
}
fn fixture() -> (Project, EditProposal) {
    let mut project = Project::new(4, 3).unwrap();
    project.rename_layer(0, "private hidden layer name").unwrap();
    project.set_pixel(0, 0, 0, [9, 8, 7, 0]).unwrap();
    project.layers[0].visible = false;
    let proposal = EditProposal { version: 1, base_revision: project.revision().unwrap(), operations: vec![
        Operation::AddLayer,
        Operation::Pixel { layer: 1, x: 2, y: 1, color: [10, 20, 30, 128] },
    ] };
    (project, proposal)
}

#[test]
fn persistent_raster_job_snapshots_inputs_verifies_outputs_and_never_replays() {
    let root = private_root();
    let (mut project, mut proposal) = fixture();
    let original = project.clone();
    let mut expected = project.clone();
    expected.apply_proposal(&proposal).unwrap();
    let queued = tasks::create(root.path(), &project, &proposal).unwrap();
    let dir = root.path().join(&queued.id);
    assert_eq!(queued.state, TaskState::Queued);
    assert_eq!(queued.history.len(), 1);
    assert!(!queued.autonomous_execution && !queued.network_used && !queued.resource_quotas_enforced);
    assert!(!queued.artifacts_verified);
    assert!(queued.outputs.is_none());
    assert_eq!(fs::metadata(&dir).unwrap().permissions().mode() & 0o777, 0o700);
    for entry in fs::read_dir(&dir).unwrap() {
        assert_eq!(entry.unwrap().metadata().unwrap().permissions().mode() & 0o077, 0);
    }
    assert!(!dir.join(OUTPUT_PROJECT).exists());
    let report = serde_json::to_string(&queued).unwrap();
    assert!(!report.contains("private hidden layer name"));
    assert!(!report.contains("pixels"));
    // Subsequent caller edits cannot alter the durable task's captured inputs.
    project.set_pixel(0, 0, 0, [255; 4]).unwrap();
    proposal.operations.clear();
    let completed = tasks::run(root.path(), &queued.id).unwrap();
    assert_eq!(completed.state, TaskState::Succeeded);
    assert!(completed.artifacts_verified);
    assert_eq!(completed.history.iter().map(|e| e.state).collect::<Vec<_>>(), [TaskState::Queued, TaskState::Running, TaskState::Succeeded]);
    assert_eq!(Project::load(&dir.join(INPUT_PROJECT)).unwrap(), original);
    assert_eq!(Project::load(&dir.join(OUTPUT_PROJECT)).unwrap(), expected);
    let exported = Project::from_png(&dir.join(OUTPUT_PNG)).unwrap();
    assert_eq!(exported.rgba().unwrap(), expected.rgba().unwrap());
    let project_bytes = fs::read(dir.join(OUTPUT_PROJECT)).unwrap();
    let png_bytes = fs::read(dir.join(OUTPUT_PNG)).unwrap();
    for report in [tasks::inspect(root.path(), &queued.id).unwrap(), tasks::run(root.path(), &queued.id).unwrap(),
        tasks::cancel(root.path(), &queued.id).unwrap(), tasks::recover(root.path(), &queued.id).unwrap()] {
        assert_eq!(report.state, TaskState::Succeeded);
        assert_eq!(report.history, completed.history);
        assert!(report.artifacts_verified);
        assert!(!report.cancel_requested);
    }
    assert_eq!(fs::read(dir.join(OUTPUT_PROJECT)).unwrap(), project_bytes);
    assert_eq!(fs::read(dir.join(OUTPUT_PNG)).unwrap(), png_bytes);
}

#[test]
fn invalid_or_stale_submission_writes_nothing() {
    let root = private_root();
    let (project, proposal) = fixture();
    let mut invalid = proposal.clone();
    invalid.base_revision = "sha256-json-v1:0000".into();
    assert!(tasks::create(root.path(), &project, &invalid).is_err());
    invalid = proposal.clone();
    invalid.version = 2;
    assert!(tasks::create(root.path(), &project, &invalid).is_err());
    invalid = proposal.clone();
    invalid.operations.push(Operation::RemoveLayer { layer: usize::MAX });
    assert!(tasks::create(root.path(), &project, &invalid).is_err());
    invalid.operations = vec![Operation::AddLayer; 1025];
    assert!(tasks::create(root.path(), &project, &invalid).is_err());
    invalid.operations = vec![Operation::RenameLayer { layer: 0, name: "x".repeat(256) }; 1024];
    assert!(tasks::create(root.path(), &project, &invalid).is_err());
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
}

#[test]
fn cancellation_before_run_is_durable_terminal_and_idempotent() {
    let root = private_root();
    let (project, proposal) = fixture();
    let id = tasks::create(root.path(), &project, &proposal).unwrap().id;
    let cancelled = tasks::cancel(root.path(), &id).unwrap();
    assert_eq!(cancelled.state, TaskState::Cancelled);
    assert!(cancelled.cancel_requested);
    assert_eq!(cancelled.history.len(), 2);
    assert_eq!(cancelled.history[1].reason, Some(TaskReason::UserCancelled));
    assert!(tasks::run(root.path(), &id).is_err());
    assert_eq!(tasks::inspect(root.path(), &id).unwrap().history, cancelled.history);
    assert_eq!(tasks::cancel(root.path(), &id).unwrap().history, cancelled.history);
    assert_eq!(tasks::recover(root.path(), &id).unwrap().history, cancelled.history);
    assert!(!root.path().join(id).join(OUTPUT_PROJECT).exists());
}

#[test]
fn failed_writes_preserve_occupied_paths_and_do_not_publish_partial_results() {
    for occupied in [OUTPUT_PROJECT, OUTPUT_PNG] {
        let root = private_root();
        let (project, proposal) = fixture();
        let id = tasks::create(root.path(), &project, &proposal).unwrap().id;
        let dir = root.path().join(&id);
        fs::write(dir.join(occupied), b"preserve existing output").unwrap();
        assert!(tasks::run(root.path(), &id).is_err());
        let failed = tasks::inspect(root.path(), &id).unwrap();
        assert_eq!(failed.state, TaskState::Failed);
        assert_eq!(failed.history[2].reason, Some(TaskReason::OutputWriteFailed));
        assert!(failed.outputs.is_none());
        assert!(!failed.artifacts_verified);
        assert_eq!(fs::read(dir.join(occupied)).unwrap(), b"preserve existing output");
        assert_eq!(Project::load(&dir.join(INPUT_PROJECT)).unwrap(), project);
        assert!(tasks::run(root.path(), &id).is_err());
        assert_eq!(tasks::recover(root.path(), &id).unwrap().state, TaskState::Failed);
    }
}

#[test]
fn changed_inputs_and_outputs_fail_integrity_checks_without_echoing_artwork() {
    let root = private_root();
    let (project, proposal) = fixture();
    let id = tasks::create(root.path(), &project, &proposal).unwrap().id;
    let dir = root.path().join(&id);
    let mut changed = project.clone();
    changed.set_pixel(0, 0, 0, [1; 4]).unwrap();
    fs::write(dir.join(INPUT_PROJECT), serde_json::to_vec(&changed).unwrap()).unwrap();
    let error = tasks::run(root.path(), &id).unwrap_err();
    assert!(!error.contains("private hidden layer name"));
    let failed = tasks::inspect(root.path(), &id).unwrap();
    assert_eq!(failed.state, TaskState::Failed);
    assert_eq!(failed.history[2].reason, Some(TaskReason::InputsChanged));
    assert!(!dir.join(OUTPUT_PROJECT).exists());
    let id = tasks::create(root.path(), &project, &proposal).unwrap().id;
    let dir = root.path().join(&id);
    tasks::run(root.path(), &id).unwrap();
    let saved = fs::read(dir.join(OUTPUT_PNG)).unwrap();
    fs::write(dir.join(OUTPUT_PNG), b"corrupted").unwrap();
    assert!(tasks::inspect(root.path(), &id).unwrap_err().contains("integrity"));
    assert!(tasks::run(root.path(), &id).is_err()); // Never rewrites corrupted completed artifacts.
    assert_eq!(fs::read(dir.join(OUTPUT_PNG)).unwrap(), b"corrupted");
    fs::write(dir.join(OUTPUT_PNG), saved).unwrap();
    assert!(tasks::inspect(root.path(), &id).unwrap().artifacts_verified);
}

#[test]
fn private_paths_ids_special_files_links_and_bounded_metadata_are_checked() {
    let root = private_root();
    let (project, proposal) = fixture();
    let id = tasks::create(root.path(), &project, &proposal).unwrap().id;
    for invalid in ["", "../task-abcdef", "task-../escape", "/tmp/task-abcdef", "task-a", "task-雪雪雪雪雪雪"] {
        assert!(tasks::inspect(root.path(), invalid).is_err());
        assert!(tasks::run(root.path(), invalid).is_err());
        assert!(tasks::cancel(root.path(), invalid).is_err());
        assert!(tasks::recover(root.path(), invalid).is_err());
    }
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o755)).unwrap();
    assert!(tasks::inspect(root.path(), &id).is_err());
    assert!(tasks::create(root.path(), &project, &proposal).is_err());
    fs::set_permissions(root.path(), fs::Permissions::from_mode(0o700)).unwrap();
    let alias = root.path().join("task-abcdef");
    std::os::unix::fs::symlink(root.path().join(&id), &alias).unwrap();
    assert!(tasks::inspect(root.path(), "task-abcdef").is_err());
    fs::remove_file(&alias).unwrap();
    let dir = root.path().join(&id);
    let snapshot = fs::read(dir.join(INPUT_PROJECT)).unwrap();
    fs::remove_file(dir.join(INPUT_PROJECT)).unwrap();
    std::os::unix::fs::symlink("proposal.json", dir.join(INPUT_PROJECT)).unwrap();
    assert!(tasks::run(root.path(), &id).is_err());
    assert_eq!(tasks::inspect(root.path(), &id).unwrap().state, TaskState::Failed);
    let id = tasks::create(root.path(), &project, &proposal).unwrap().id;
    let dir = root.path().join(&id);
    let linked = root.path().join("hardlink");
    fs::hard_link(dir.join(INPUT_PROJECT), &linked).unwrap();
    assert!(tasks::run(root.path(), &id).is_err());
    fs::remove_file(linked).unwrap();
    let id = tasks::create(root.path(), &project, &proposal).unwrap().id;
    let dir = root.path().join(&id);
    fs::write(dir.join("task.json"), vec![b' '; tasks::MAX_METADATA_BYTES as usize + 1]).unwrap();
    assert!(tasks::inspect(root.path(), &id).is_err());
    assert_eq!(snapshot, serde_json::to_vec(&project).unwrap());
    let id = tasks::create(root.path(), &project, &proposal).unwrap().id;
    let dir = root.path().join(&id);
    fs::remove_file(dir.join(INPUT_PROJECT)).unwrap();
    let path = std::ffi::CString::new(dir.join(INPUT_PROJECT).as_os_str().as_encoded_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
    assert!(tasks::run(root.path(), &id).is_err()); // O_NONBLOCK prevents a hang.
}

#[test]
fn concurrent_runners_commit_only_one_three_event_history() {
    let root = private_root();
    let (project, proposal) = fixture();
    let id = tasks::create(root.path(), &project, &proposal).unwrap().id;
    let start = Arc::new(Barrier::new(8));
    let mut runners = Vec::new();
    for _ in 0..8 {
        let start = Arc::clone(&start);
        let root = root.path().to_owned();
        let id = id.clone();
        runners.push(std::thread::spawn(move || { start.wait(); tasks::run(&root, &id).is_ok() }));
    }
    assert!(runners.into_iter().map(|runner| runner.join().unwrap()).filter(|ok| *ok).count() >= 1);
    let done = tasks::inspect(root.path(), &id).unwrap();
    assert_eq!(done.state, TaskState::Succeeded);
    assert_eq!(done.history.len(), 3);
    assert!(done.artifacts_verified);
    assert_eq!(fs::read_dir(root.path().join(id)).unwrap().count(), 9); // Four immutable inputs/metadata, three events, two outputs.
}

fn cli(args: &[&str]) -> serde_json::Value {
    let output = Command::new(env!("CARGO_BIN_EXE_perveance-cli")).args(args).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn cli_tasks_survive_separate_processes_and_recover_a_simulated_interruption() {
    let inputs = private_root();
    let root = private_root();
    let (project, proposal) = fixture();
    let input = inputs.path().join("project.json");
    let proposal_path = inputs.path().join("proposal.json");
    project.save_new(&input).unwrap();
    fs::write(&proposal_path, serde_json::to_vec(&proposal).unwrap()).unwrap();
    let root_arg = root.path().to_str().unwrap();
    let create = || cli(&["task-create", input.to_str().unwrap(), proposal_path.to_str().unwrap(), root_arg]);
    let queued = create();
    let id = queued["task"]["id"].as_str().unwrap();
    assert_eq!(queued["task"]["state"], "queued");
    assert_eq!(cli(&["task-inspect", root_arg, id])["task"]["state"], "queued");
    assert_eq!(cli(&["task-run", root_arg, id])["task"]["state"], "succeeded");
    assert_eq!(cli(&["task-inspect", root_arg, id])["task"]["artifacts_verified"], true);
    let queued = create();
    let id = queued["task"]["id"].as_str().unwrap();
    assert_eq!(cli(&["task-cancel", root_arg, id])["task"]["state"], "cancelled");
    let queued = create();
    let id = queued["task"]["id"].as_str().unwrap();
    let directory = root.path().join(id);
    fs::write(directory.join("event-001.json"), br#"{"version":1,"sequence":1,"state":"running","reason":null,"outputs":null}"#).unwrap();
    fs::set_permissions(directory.join("event-001.json"), fs::Permissions::from_mode(0o600)).unwrap();
    fs::write(directory.join(OUTPUT_PROJECT), b"unpublished partial output").unwrap();
    assert_eq!(cli(&["task-inspect", root_arg, id])["task"]["state"], "running");
    let recovered = cli(&["task-recover", root_arg, id]);
    assert_eq!(recovered["task"]["state"], "interrupted");
    assert_eq!(recovered["task"]["outputs"], serde_json::Value::Null);
    assert_eq!(cli(&["task-recover", root_arg, id])["task"]["history"], recovered["task"]["history"]);
    assert_eq!(fs::read(directory.join(OUTPUT_PROJECT)).unwrap(), b"unpublished partial output");
    let refused = Command::new(env!("CARGO_BIN_EXE_perveance-cli")).args(["task-run", root_arg, id]).output().unwrap();
    assert!(!refused.status.success());
    let caps = cli(&["capabilities"]);
    for command in ["task-create", "task-inspect", "task-run", "task-cancel", "task-recover"] {
        assert!(caps["operations"].as_array().unwrap().iter().any(|value| value == command));
    }
    assert_eq!(caps["persistent_raster_tasks"]["autonomous_execution"], false);
    assert_eq!(caps["autonomy_intake"]["task_created"], false);
}

#[test]
fn forced_cli_termination_releases_the_lock_and_requires_explicit_recovery() {
    use std::{process::Stdio, time::{Duration, Instant}};
    struct ChildGuard(std::process::Child);
    impl Drop for ChildGuard {
        fn drop(&mut self) { let _ = self.0.kill(); let _ = self.0.wait(); }
    }
    let root = private_root();
    // A sizeable but valid snapshot makes the running boundary observable before
    // this trusted CLI finishes. No generated code, sleeps or test hooks are
    // inserted into the production runner, and this does not test power loss.
    let mut project = Project::new(256, 256).unwrap();
    while project.layers.len() < 16 { project.add_layer().unwrap(); }
    for layer in &mut project.layers { layer.pixels.fill([255; 4]); }
    let proposal = EditProposal { version: 1, base_revision: project.revision().unwrap(),
        operations: vec![Operation::Pixel { layer: 0, x: 0, y: 0, color: [1, 2, 3, 4] }] };
    let id = tasks::create(root.path(), &project, &proposal).unwrap().id;
    let directory = root.path().join(&id);
    let mut child = ChildGuard(Command::new(env!("CARGO_BIN_EXE_perveance-cli"))
        .args(["task-run", root.path().to_str().unwrap(), &id])
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap());
    let deadline = Instant::now() + Duration::from_secs(60);
    while !directory.join("event-001.json").exists() {
        assert!(child.0.try_wait().unwrap().is_none(), "runner exited before the running boundary");
        assert!(Instant::now() < deadline, "runner did not reach the running boundary");
        std::thread::sleep(Duration::from_millis(2));
    }
    child.0.kill().unwrap();
    assert!(!child.0.wait().unwrap().success());
    let report = tasks::inspect(root.path(), &id).unwrap();
    assert_eq!(report.state, TaskState::Running);
    assert!(report.outputs.is_none());
    assert!(tasks::run(root.path(), &id).is_err());
    let recovered = cli(&["task-recover", root.path().to_str().unwrap(), &id]);
    assert_eq!(recovered["task"]["state"], "interrupted");
    assert_eq!(recovered["task"]["outputs"], serde_json::Value::Null);
    assert_eq!(Project::load(&directory.join(INPUT_PROJECT)).unwrap(), project);
    assert_eq!(tasks::recover(root.path(), &id).unwrap().history.len(), 3);
}
