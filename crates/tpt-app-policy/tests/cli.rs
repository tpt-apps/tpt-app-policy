use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

fn example(relative: &str) -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(relative)
        .to_string_lossy()
        .into_owned()
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tpt-policy"))
        .args(args)
        .output()
        .expect("tpt-policy runs")
}

fn code(output: &Output) -> i32 {
    output.status.code().expect("process exits normally")
}

fn stdout_json(output: &Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout).expect("stdout is JSON")
}

#[test]
fn version_flag_prints_version() {
    let out = run(&["--version"]);
    assert_eq!(code(&out), 0);
    assert!(String::from_utf8_lossy(&out.stdout).contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn check_matches_golden_expense_output() {
    let out = run(&[
        "check",
        &example("expense/expense.yaml"),
        &example("expense/expense.json"),
    ]);
    assert_eq!(code(&out), 10, "approval_required exits 10");

    let expected: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(example("expense/expense.expected.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(stdout_json(&out), expected);
}

#[test]
fn check_output_is_byte_identical_across_runs() {
    let args = [
        "check",
        &example("expense/expense.yaml"),
        &example("expense/expense.json"),
    ];
    assert_eq!(run(&args).stdout, run(&args).stdout);
}

#[test]
fn run_reads_policy_decision_from_stdin() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_tpt-policy"))
        .args(["run", &example("purchasing/purchasing.yaml")])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(br#"{"amount": 500}"#)
        .unwrap();
    let out = child.wait_with_output().unwrap();

    assert_eq!(code(&out), 0, "approved exits 0");
    let json = stdout_json(&out);
    assert_eq!(json["decision"], "approved");
    assert_eq!(
        json["matched_rules"],
        serde_json::json!(["purchase-under-limit"])
    );
}

#[test]
fn rejected_decision_exits_30() {
    let dir = std::env::temp_dir().join(format!("tpt-policy-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let policy = dir.join("reject.yaml");
    std::fs::write(
        &policy,
        "policy: t\nrules:\n  - id: blocked\n    when:\n      supplier.status: blocked\n    then:\n      decision: rejected\n",
    )
    .unwrap();
    let input = dir.join("input.json");
    std::fs::write(&input, r#"{"supplier": {"status": "blocked"}}"#).unwrap();

    let out = run(&["check", policy.to_str().unwrap(), input.to_str().unwrap()]);
    assert_eq!(code(&out), 30);
    assert_eq!(stdout_json(&out)["decision"], "rejected");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn explain_text_on_expense_example() {
    let out = run(&[
        "explain",
        &example("expense/expense.yaml"),
        &example("expense/expense.json"),
    ]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("decision: approval_required"));
    assert!(text.contains("approvers: manager, finance"));
    assert!(text.contains("explanations:"));
    assert!(text.contains("failed rules:"));
}

#[test]
fn validate_accepts_good_policy() {
    let out = run(&["validate", &example("expense/expense.yaml")]);
    assert_eq!(code(&out), 0);
}

#[test]
fn validate_rejects_bad_policy_with_diagnostics() {
    let out = run(&["validate", &example("invalid/unknown-operator.yaml")]);
    assert_eq!(code(&out), 2);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("unknown operator"));
    assert!(err.contains("why:"));
    assert!(err.contains("fix:"));
}

#[test]
fn test_command_passes_for_purchasing_examples() {
    let out = run(&["test", &example("purchasing/purchasing.yaml")]);
    assert_eq!(code(&out), 0);
    assert!(String::from_utf8_lossy(&out.stdout).contains("3 tests passed"));
}

#[test]
fn missing_policy_file_exits_1() {
    let out = run(&["validate", &example("does-not-exist.yaml")]);
    assert_eq!(code(&out), 1);
}

#[test]
fn bad_input_json_exits_3() {
    let dir = std::env::temp_dir().join(format!("tpt-policy-badjson-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let input = dir.join("bad.json");
    std::fs::write(&input, "{not json").unwrap();

    let out = run(&[
        "check",
        &example("expense/expense.yaml"),
        input.to_str().unwrap(),
    ]);
    assert_eq!(code(&out), 3);

    std::fs::remove_dir_all(&dir).ok();
}
