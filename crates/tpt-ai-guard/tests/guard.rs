use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn example(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/ai-guard")
        .join(relative)
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tpt-ai-guard"))
        .args(args)
        .output()
        .expect("tpt-ai-guard runs")
}

fn code(output: &Output) -> i32 {
    output.status.code().expect("process exits normally")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tpt-ai-guard-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn policy() -> PathBuf {
    example("customer-actions.policy.yaml")
}

fn check(request: &Path) -> Output {
    run(&[
        "check",
        request.to_str().unwrap(),
        "--policy",
        policy().to_str().unwrap(),
    ])
}

fn verdict(output: &Output) -> serde_json::Value {
    serde_json::from_str(&stdout(output)).expect("JSON verdict")
}

#[test]
fn large_refund_requires_approval_with_the_reason() {
    let out = check(&example("requests/refund-large.json"));
    assert_eq!(code(&out), 10);
    let v = verdict(&out);
    assert_eq!(v["decision"], "REQUIRE_APPROVAL");
    assert_eq!(v["reason"], "refund exceeds $5,000 limit");
}

#[test]
fn small_refund_is_allowed_and_exits_0() {
    let out = check(&example("requests/refund-small.json"));
    assert_eq!(code(&out), 0);
    assert_eq!(verdict(&out)["decision"], "ALLOW");
}

#[test]
fn deletion_is_denied_and_exits_30() {
    let out = check(&example("requests/delete.json"));
    assert_eq!(code(&out), 30);
    assert_eq!(verdict(&out)["decision"], "DENY");
}

#[test]
fn unknown_action_is_denied_by_default() {
    let out = check(&example("requests/unknown.json"));
    assert_eq!(code(&out), 30);
    let v = verdict(&out);
    assert_eq!(v["decision"], "DENY");
    assert!(v["reason"].as_str().unwrap().contains("denied by default"));
}

#[test]
fn text_format_shows_the_decision_first() {
    let out = run(&[
        "check",
        example("requests/refund-large.json").to_str().unwrap(),
        "--policy",
        policy().to_str().unwrap(),
        "--format",
        "text",
    ]);
    assert_eq!(code(&out), 10);
    assert!(stdout(&out).starts_with("decision: REQUIRE_APPROVAL"));
}

#[test]
fn same_request_gives_identical_output() {
    let first = check(&example("requests/refund-large.json"));
    let second = check(&example("requests/refund-large.json"));
    assert_eq!(first.stdout, second.stdout);
}

#[test]
fn request_without_an_action_exits_3() {
    let dir = scratch("no-action");
    let file = dir.join("req.json");
    fs::write(&file, "{\"amount\": 10}").unwrap();
    let out = check(&file);
    assert_eq!(code(&out), 3);
    assert!(String::from_utf8_lossy(&out.stderr).contains("'action'"));
}

#[test]
fn request_that_is_not_an_object_exits_3() {
    let dir = scratch("array");
    let file = dir.join("req.json");
    fs::write(&file, "[\"refund_customer\"]").unwrap();
    assert_eq!(code(&check(&file)), 3);
}

#[test]
fn policy_with_an_unknown_decision_exits_2() {
    let dir = scratch("bad-policy");
    let file = dir.join("p.yaml");
    fs::write(&file, "rules:\n  - action: x\n    decision: perhaps\n").unwrap();
    let out = run(&["validate", "--policy", file.to_str().unwrap()]);
    assert_eq!(code(&out), 2);
    assert!(String::from_utf8_lossy(&out.stderr).contains("invalid action policy"));
}

#[test]
fn spec_style_bare_list_is_accepted() {
    let dir = scratch("bare");
    let file = dir.join("p.yaml");
    fs::write(
        &file,
        "- action: refund_customer\n  when:\n    amount:\n      gt: 5000\n  decision:\n    require_approval: true\n  reason: refund exceeds $5,000 limit\n",
    )
    .unwrap();
    let req = dir.join("r.json");
    fs::write(&req, "{\"action\": \"refund_customer\", \"amount\": 7500}").unwrap();
    let out = run(&[
        "check",
        req.to_str().unwrap(),
        "--policy",
        file.to_str().unwrap(),
    ]);
    assert_eq!(code(&out), 10);
    assert_eq!(verdict(&out)["reason"], "refund exceeds $5,000 limit");
}

#[test]
fn validate_accepts_the_example() {
    let out = run(&["validate", "--policy", policy().to_str().unwrap()]);
    assert_eq!(code(&out), 0);
    assert!(stdout(&out).contains("(4 rules)"));
}

#[test]
fn doctor_passes() {
    let out = run(&["doctor"]);
    assert_eq!(code(&out), 0);
    assert!(stdout(&out).contains("all checks passed"));
}
