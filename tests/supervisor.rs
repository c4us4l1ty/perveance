use perveance::{supervisor::{self, Budgets, IntakePolicy, TaskRequest}, Project};
use std::fs;

fn request(project: &Project) -> TaskRequest {
    TaskRequest { version: 1, base_revision: project.revision().unwrap(),
        prompt: "private art request".into(), budgets: Budgets {
            wall_seconds: 10, cpu_seconds: 10, memory_mib: 32, scratch_mib: 32,
            processes: 2, model_calls: 1, model_cost_microusd: 1000,
        } }
}

#[test]
fn valid_request_is_always_blocked_and_never_echoes_prompt() {
    let project = Project::new(2, 2).unwrap();
    let req = request(&project);
    let policy = IntakePolicy::default();
    let before = project.clone();
    let report = supervisor::inspect_request(&project, &req, &policy).unwrap();
    assert_eq!(report.status, "blocked");
    assert_eq!(report.revision, req.base_revision);
    assert!(!report.autonomous_execution && !report.budgets_enforced && !report.persistent_task_created);
    assert!(report.blocking_gates.contains(&"verified_sandbox"));
    assert!(report.blocking_gates.contains(&"resource_enforcement"));
    assert!(!serde_json::to_string(&report).unwrap().contains(&req.prompt));
    assert_eq!(project, before);
    // Even ceilings set to the maximum possible integer cannot create a permit.
    let max = Budgets { wall_seconds: u64::MAX, cpu_seconds: u64::MAX, memory_mib: u64::MAX,
        scratch_mib: u64::MAX, processes: u64::MAX, model_calls: u64::MAX, model_cost_microusd: u64::MAX };
    let report = supervisor::inspect_request(&project, &req, &IntakePolicy {version: 1, maximum: max}).unwrap();
    assert!(!report.autonomous_execution);
}

#[test]
fn each_budget_is_checked_against_policy_and_zero_rules() {
    let project = Project::new(1, 1).unwrap();
    let policy = IntakePolicy::default();
    let mut req = request(&project);
    req.budgets = policy.maximum.clone();
    assert!(supervisor::inspect_request(&project, &req, &policy).is_ok());
    for field in 0..7 {
        let mut excessive = request(&project);
        let (v, max) = match field {
            0 => (&mut excessive.budgets.wall_seconds, policy.maximum.wall_seconds),
            1 => (&mut excessive.budgets.cpu_seconds, policy.maximum.cpu_seconds),
            2 => (&mut excessive.budgets.memory_mib, policy.maximum.memory_mib),
            3 => (&mut excessive.budgets.scratch_mib, policy.maximum.scratch_mib),
            4 => (&mut excessive.budgets.processes, policy.maximum.processes),
            5 => (&mut excessive.budgets.model_calls, policy.maximum.model_calls),
            _ => (&mut excessive.budgets.model_cost_microusd, policy.maximum.model_cost_microusd),
        };
        *v = max + 1;
        assert!(supervisor::inspect_request(&project, &excessive, &policy).is_err());
    }
    for field in 0..5 {
        let mut zero = request(&project);
        match field {
            0 => zero.budgets.wall_seconds = 0,
            1 => zero.budgets.cpu_seconds = 0,
            2 => zero.budgets.memory_mib = 0,
            3 => zero.budgets.scratch_mib = 0,
            _ => zero.budgets.processes = 0,
        }
        assert!(supervisor::inspect_request(&project, &zero, &policy).is_err());
    }
    req.budgets.model_calls = 0;
    req.budgets.model_cost_microusd = 0;
    assert!(supervisor::inspect_request(&project, &req, &policy).is_ok());
    let mut tight = policy.clone();
    tight.maximum.wall_seconds = 0;
    assert!(supervisor::inspect_request(&project, &req, &tight).is_err());
}

#[test]
fn rejects_stale_invalid_or_oversized_inputs() {
    let mut project = Project::new(1, 1).unwrap();
    let mut req = request(&project);
    let policy = IntakePolicy::default();
    project.layers[0].visible = false;
    assert!(supervisor::inspect_request(&project, &req, &policy).is_err());
    req.base_revision = project.revision().unwrap();
    req.version = 2;
    assert!(supervisor::inspect_request(&project, &req, &policy).is_err());
    req.version = 1;
    let mut bad_policy = policy.clone(); bad_policy.version = 2;
    assert!(supervisor::inspect_request(&project, &req, &bad_policy).is_err());
    for text in [" \n".to_string(), "é".repeat(2001)] {
        req.prompt = text;
        assert!(supervisor::inspect_request(&project, &req, &policy).is_err());
    }
    req.prompt = "é".repeat(2000);
    assert!(supervisor::inspect_request(&project, &req, &policy).is_ok());
    project.layers[0].pixels.clear();
    assert!(supervisor::inspect_request(&project, &req, &policy).is_err());
}

#[test]
fn json_files_are_bounded_strict_and_errors_do_not_echo_input() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("request.json");
    let p = Project::new(1, 1).unwrap();
    let r = request(&p);
    let valid = serde_json::json!({"version": 1, "base_revision": r.base_revision, "prompt": r.prompt, "budgets": r.budgets});
    fs::write(&file, serde_json::to_vec(&valid).unwrap()).unwrap();
    assert!(TaskRequest::load(&file).is_ok());
    for malformed in [
        r#"{"private-secret": 123}"#.to_string(),
        r#"{"version":1,"version":1}"#.to_string(),
        "x".repeat(supervisor::MAX_REQUEST_BYTES as usize + 1),
    ] {
        fs::write(&file, malformed).unwrap();
        let err = TaskRequest::load(&file).err().unwrap();
        assert!(!err.contains("private-secret"));
    }
    let mut extra = valid;
    extra["permit"] = true.into();
    fs::write(&file, serde_json::to_vec(&extra).unwrap()).unwrap();
    assert!(TaskRequest::load(&file).is_err());
    let policy = dir.path().join("policy.json");
    fs::write(&policy, serde_json::to_vec(&IntakePolicy::default()).unwrap()).unwrap();
    assert_eq!(IntakePolicy::load(&policy).unwrap(), IntakePolicy::default());
    fs::write(&policy, vec![b' '; supervisor::MAX_POLICY_BYTES as usize + 1]).unwrap();
    assert!(IntakePolicy::load(&policy).is_err());
}
