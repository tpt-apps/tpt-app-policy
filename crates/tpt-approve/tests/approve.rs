use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn example(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/approval")
        .join(relative)
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tpt-approve"))
        .args(args)
        .output()
        .expect("tpt-approve runs")
}

fn code(output: &Output) -> i32 {
    output.status.code().expect("process exits normally")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tpt-approve-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn check(request: &Path, rules: &Path) -> Output {
    run(&[
        "check",
        request.to_str().unwrap(),
        "--rules",
        rules.to_str().unwrap(),
    ])
}

fn rules() -> PathBuf {
    example("expense.approval.yaml")
}

#[test]
fn automatic_band_exits_0() {
    let out = check(&example("requests/small.json"), &rules());
    assert_eq!(code(&out), 0);
    let v: serde_json::Value = serde_json::from_str(&stdout(&out)).unwrap();
    assert_eq!(v["decision"], "approved");
    assert_eq!(v["policy"]["name"], "expense-approval");
}

#[test]
fn manager_band_exits_10_and_names_the_manager() {
    let out = check(&example("requests/manager.json"), &rules());
    assert_eq!(code(&out), 10);
    let v: serde_json::Value = serde_json::from_str(&stdout(&out)).unwrap();
    assert_eq!(v["decision"], "approval_required");
    assert_eq!(v["approvers"], serde_json::json!(["manager"]));
}

#[test]
fn director_band_exits_10_with_the_requirement() {
    let out = check(&example("requests/director.json"), &rules());
    assert_eq!(code(&out), 10);
    let v: serde_json::Value = serde_json::from_str(&stdout(&out)).unwrap();
    assert_eq!(v["approvers"], serde_json::json!(["director"]));
    assert_eq!(v["requirements"], serde_json::json!(["Quote attached"]));
}

#[test]
fn request_without_the_field_gets_review_exit_20() {
    let out = check(&example("requests/no-amount.json"), &rules());
    assert_eq!(code(&out), 20);
    let v: serde_json::Value = serde_json::from_str(&stdout(&out)).unwrap();
    assert_eq!(v["decision"], "review");
}

#[test]
fn text_format_is_readable() {
    let out = run(&[
        "check",
        example("requests/director.json").to_str().unwrap(),
        "--rules",
        rules().to_str().unwrap(),
        "--format",
        "text",
    ]);
    assert_eq!(code(&out), 10);
    assert!(stdout(&out).contains("approvers: director"));
}

#[test]
fn json_output_is_identical_across_runs() {
    let first = check(&example("requests/manager.json"), &rules());
    let second = check(&example("requests/manager.json"), &rules());
    assert_eq!(first.stdout, second.stdout);
}

#[test]
fn validate_example_has_no_gaps() {
    let out = run(&["validate", "--rules", rules().to_str().unwrap()]);
    assert_eq!(code(&out), 0);
    assert!(stdout(&out).contains("no gaps found"), "{}", stdout(&out));
}

#[test]
fn validate_reports_a_gap_but_still_exits_0() {
    let dir = scratch("gap");
    let file = dir.join("gap.yaml");
    std::fs::write(
        &file,
        "approval: gap\nrules:\n  - when: {amount: \"<1000\"}\n    decision: automatic\n  - when: {amount: \">2000\"}\n    approver: director\n",
    )
    .unwrap();
    let out = run(&["validate", "--rules", file.to_str().unwrap()]);
    assert_eq!(code(&out), 0);
    assert!(
        stdout(&out).contains("warning: no rule covers amount strictly between 1000 and 2000"),
        "{}",
        stdout(&out)
    );
}

#[test]
fn invalid_rules_exit_2_with_the_rule_named() {
    let dir = scratch("invalid");
    let file = dir.join("bad.yaml");
    std::fs::write(
        &file,
        "approval: bad\nrules:\n  - id: band-one\n    when: {amount: \"<abc\"}\n    decision: automatic\n",
    )
    .unwrap();
    let out = run(&["validate", "--rules", file.to_str().unwrap()]);
    assert_eq!(code(&out), 2);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("band-one"), "{stderr}");
    assert!(stderr.contains("needs a number"), "{stderr}");
}

#[test]
fn request_that_is_not_an_object_exits_3() {
    let dir = scratch("array");
    let file = dir.join("req.json");
    std::fs::write(&file, "[1, 2]").unwrap();
    assert_eq!(code(&check(&file, &rules())), 3);
}

#[test]
fn doctor_passes() {
    let out = run(&["doctor"]);
    assert_eq!(code(&out), 0);
    assert!(stdout(&out).contains("all checks passed"));
}
