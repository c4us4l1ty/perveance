#![cfg(target_os = "linux")]
use perveance::{bounded_worker::Limits, worker_io::VerifiedBundle,
    worker_tasks::{self, State}, EditProposal, Operation, Project};
use std::{fs, os::unix::fs::{PermissionsExt, symlink}, process::Command};
fn root() -> tempfile::TempDir { tempfile::Builder::new().permissions(fs::Permissions::from_mode(0o700)).tempdir().unwrap() }
fn fixture() -> (Project, EditProposal) {
    let mut project = Project::new(4, 3).unwrap();
    project.layers[0].name = "private hidden layer".into(); project.layers[0].visible = false;
    project.set_pixel(0, 0, 0, [9, 8, 7, 0]).unwrap();
    let proposal = EditProposal { version: 1, base_revision: project.revision().unwrap(),
        operations: vec![Operation::Pixel { layer: 0, x: 2, y: 1, color: [10, 20, 30, 128] }] };
    (project, proposal)
}
#[test]
fn captured_pixel_jobs_are_private_metadata_only_and_cancel_without_a_backend() {
    let root = root(); let (mut project, mut proposal) = fixture();
    let captured = project.clone();
    let queued = worker_tasks::create(root.path(), &project, &proposal, &Limits::default()).unwrap();
    let dir = root.path().join(&queued.id);
    assert_eq!(queued.state, State::Queued);
    assert_eq!(queued.ledger.as_ref().unwrap().reserved_calls, 0);
    assert!(queued.synthetic_fixture_only && !queued.resource_quotas_enforced && !queued.autonomous_execution);
    let report = serde_json::to_string(&queued).unwrap();
    assert!(!report.contains("private hidden layer") && !report.contains("pixels\""));
    assert_eq!(fs::metadata(&dir).unwrap().permissions().mode() & 0o777, 0o700);
    project.layers[0].pixels.fill([255; 4]); proposal.operations.clear();
    assert_eq!(Project::load(&dir.join("input.perveance.json")).unwrap(), captured);
    let receipt = worker_tasks::cancel(root.path(), &queued.id).unwrap();
    assert!(receipt.request_recorded && !receipt.already_terminal);
    let report = worker_tasks::inspect(root.path(), &queued.id).unwrap();
    assert_eq!(report.state, State::Cancelled); assert!(report.cancel_requested);
    assert_eq!(report.history.len(), 2); assert!(report.outputs.is_none());
    assert_eq!(report.ledger.unwrap().reserved_calls, 0);
    assert!(worker_tasks::cancel(root.path(), &queued.id).unwrap().already_terminal);
    assert!(worker_tasks::run(root.path(), &queued.id, root.path()).is_err());
    assert_eq!(worker_tasks::recover(root.path(), &queued.id, root.path()).unwrap().state, State::Cancelled);
    assert!(!dir.join("result").exists());
}
#[test]
fn invalid_stale_nonpixel_and_oversized_proposals_write_nothing() {
    let root = root(); let (project, proposal) = fixture();
    for operations in [vec![], vec![Operation::AddLayer],
        vec![Operation::Pixel { layer: 0, x: 4, y: 0, color: [0; 4] }],
        vec![Operation::Pixel { layer: 0, x: 0, y: 0, color: [0; 4] }; 1025]] {
        let mut proposal = proposal.clone(); proposal.operations = operations;
        assert!(worker_tasks::create(root.path(), &project, &proposal, &Limits::default()).is_err());
    }
    let mut stale = proposal.clone(); stale.base_revision = "private-stale-marker".into();
    let error = worker_tasks::create(root.path(), &project, &stale, &Limits::default()).unwrap_err();
    assert!(!error.contains("private-stale-marker"));
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
}
#[test]
fn malformed_partial_changed_binding_and_extra_events_fail_closed_without_echoing() {
    let root = root(); let (project, proposal) = fixture();
    let id = worker_tasks::create(root.path(), &project, &proposal, &Limits::default()).unwrap().id;
    let journal = root.path().join(&id).join("worker-task.v1");
    let original = fs::read(&journal).unwrap();
    for change in 0..4 {
        let mut bytes = original.clone();
        match change {
            0 => { bytes.pop(); },
            1 => bytes.extend_from_slice(b"{\"sequence\":3}\n"),
            2 => { let text = String::from_utf8(bytes).unwrap(); bytes = text.replace("fixed_pixels_offline_v1", "private-header-marker").into_bytes(); },
            _ => { let mut lines = String::from_utf8(bytes).unwrap().lines().map(str::to_owned).collect::<Vec<_>>();
                let mut event: serde_json::Value = serde_json::from_str(&lines[1]).unwrap();
                event["private-field-marker"] = true.into(); lines[1] = event.to_string(); bytes = (lines.join("\n")+"\n").into_bytes(); },
        }
        fs::write(&journal, bytes).unwrap();
        let error = worker_tasks::inspect(root.path(), &id).unwrap_err();
        assert!(!error.contains("private-header-marker") && !error.contains("private-field-marker"));
        assert!(worker_tasks::run(root.path(), &id, root.path()).is_err());
        assert!(worker_tasks::recover(root.path(), &id, root.path()).is_err());
    }
    fs::write(journal, original).unwrap();
    for id in ["../escape", "worker-abc", "task-abcdef", "", "worker-雪雪雪雪雪雪"] {
        assert!(worker_tasks::inspect(root.path(), id).is_err()); assert!(worker_tasks::cancel(root.path(), id).is_err());
    }
    let marker = root.path().join(&id).join("cancel.requested");
    symlink("input.perveance.json", &marker).unwrap();
    assert!(worker_tasks::inspect(root.path(), &id).is_err()); assert!(worker_tasks::cancel(root.path(), &id).is_err());
}
#[test]
fn unsupported_backend_never_launches_or_spends_and_cli_discovery_stays_disabled() {
    let root = root(); let (project, proposal) = fixture();
    let id = worker_tasks::create(root.path(), &project, &proposal, &Limits::default()).unwrap().id;
    assert!(worker_tasks::run(root.path(), &id, root.path()).is_err());
    let report = worker_tasks::inspect(root.path(), &id).unwrap();
    assert_eq!(report.state, State::Queued); assert_eq!(report.ledger.unwrap().reserved_calls, 0);
    assert!(!root.path().join(&id).join("result").exists());
    let result = Command::new(env!("CARGO_BIN_EXE_perveance-cli")).arg("capabilities").output().unwrap();
    assert!(result.status.success());
    let value: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(value["fixed_pixel_worker_tasks"]["autonomous_execution"], false);
    assert_eq!(value["fixed_pixel_worker_tasks"]["production_security_verified"], false);
    for op in ["worker-task-create", "worker-task-run", "worker-task-cancel", "worker-task-inspect", "worker-task-recover"] {
        assert!(value["operations"].as_array().unwrap().iter().any(|item| item == op));
    }
}
#[test]
fn candidate_pixels_cannot_bless_hidden_data_changes_or_extra_schema() {
    let (project, proposal) = fixture(); let mut expected = project.clone(); expected.apply_proposal(&proposal).unwrap();
    let bytes = serde_json::to_vec(&expected).unwrap();
    assert!(VerifiedBundle::from_candidate_project(&bytes, &expected).is_ok());
    for field in 0..3 {
        let mut changed = expected.clone();
        match field { 0 => changed.layers[0].name = "unapproved".into(),
            1 => changed.layers[0].pixels[0] = [1, 2, 3, 0], _ => changed.layers[0].visible = true }
        assert!(VerifiedBundle::from_candidate_project(&serde_json::to_vec(&changed).unwrap(), &expected).is_err());
    }
    let mut value = serde_json::to_value(&expected).unwrap(); value["self_verified"] = true.into();
    assert!(VerifiedBundle::from_candidate_project(&serde_json::to_vec(&value).unwrap(), &expected).is_err());
}
