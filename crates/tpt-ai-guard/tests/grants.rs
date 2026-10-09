use tpt_ai_guard::{decide_with_grants, parse_action_policy, parse_grants};

const POLICY: &str = "rules:\n  - action: read_file\n    decision: allow\n  - action: call_api\n    decision: allow\n  - action: run\n    decision: allow\n";

const GRANTS: &str =
    "fs_read: [\"/data/exports\"]\nnet_connect: [\"api.example.com\"]\nprocess_spawn: [\"git\"]\n";

fn decide(request: serde_json::Value) -> String {
    let policy = parse_action_policy(POLICY).unwrap();
    let grants = parse_grants(GRANTS).unwrap();
    decide_with_grants(&policy, Some(&grants), &request)
        .unwrap()
        .decision
        .to_string()
}

#[test]
fn path_inside_the_grant_is_allowed() {
    assert_eq!(
        decide(serde_json::json!({"action": "read_file", "path": "/data/exports/q1.csv"})),
        "ALLOW"
    );
}

#[test]
fn path_outside_the_grant_is_denied_even_when_the_rule_allows() {
    let v = decide_with_grants(
        &parse_action_policy(POLICY).unwrap(),
        Some(&parse_grants(GRANTS).unwrap()),
        &serde_json::json!({"action": "read_file", "path": "/etc/passwd"}),
    )
    .unwrap();
    assert_eq!(v.decision, "DENY");
    assert!(v.reason.contains("/etc/passwd"));
    assert!(v.reason.contains("fs_read"));
}

#[test]
fn path_prefix_does_not_match_a_sibling_folder() {
    assert_eq!(
        decide(serde_json::json!({"action": "read_file", "path": "/data/exports-old/x.csv"})),
        "DENY"
    );
}

#[test]
fn host_must_be_granted() {
    assert_eq!(
        decide(serde_json::json!({"action": "call_api", "host": "api.example.com"})),
        "ALLOW"
    );
    assert_eq!(
        decide(serde_json::json!({"action": "call_api", "host": "evil.example.org"})),
        "DENY"
    );
}

#[test]
fn program_must_be_granted() {
    assert_eq!(
        decide(serde_json::json!({"action": "run", "program": "git"})),
        "ALLOW"
    );
    assert_eq!(
        decide(serde_json::json!({"action": "run", "program": "powershell"})),
        "DENY"
    );
}

#[test]
fn request_without_resources_is_not_affected_by_grants() {
    assert_eq!(decide(serde_json::json!({"action": "read_file"})), "ALLOW");
}

#[test]
fn without_grants_resources_are_not_checked() {
    let policy = parse_action_policy(POLICY).unwrap();
    let v = tpt_ai_guard::decide(
        &policy,
        &serde_json::json!({"action": "read_file", "path": "/etc/passwd"}),
    )
    .unwrap();
    assert_eq!(v.decision, "ALLOW");
}

#[test]
fn unknown_grant_key_is_refused() {
    assert!(parse_grants("fs_write: [\"/tmp\"]\n").is_err());
}
