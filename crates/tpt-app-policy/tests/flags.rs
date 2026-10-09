//! Tests for the common flags (--output, --quiet), the input size limit, and
//! the input fingerprint in the output.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn example(relative: &str) -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(relative)
        .to_string_lossy()
        .into_owned()
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tpt-flags-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    dir
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

#[test]
fn check_output_flag_writes_file_and_leaves_stdout_empty() {
    let dir = scratch("output");
    let target = dir.join("decision.json");
    let out = run(&[
        "check",
        &example("expense/expense.yaml"),
        &example("expense/expense.json"),
        "--output",
        target.to_str().unwrap(),
    ]);
    assert_eq!(code(&out), 10);
    assert!(
        out.stdout.is_empty(),
        "nothing on stdout when --output is set"
    );

    let written = std::fs::read_to_string(&target).expect("output file exists");
    let expected = std::fs::read_to_string(example("expense/expense.expected.json")).unwrap();
    assert_eq!(written, expected);
}

#[test]
fn output_to_missing_folder_is_io_error() {
    let dir = scratch("missing");
    let target = dir.join("no-such-folder").join("decision.json");
    let out = run(&[
        "check",
        &example("expense/expense.yaml"),
        &example("expense/expense.json"),
        "--output",
        target.to_str().unwrap(),
    ]);
    assert_eq!(code(&out), 1);
    assert!(String::from_utf8_lossy(&out.stderr).contains("cannot write output"));
}

#[test]
fn test_quiet_prints_only_the_summary_on_success() {
    let out = run(&["test", &example("purchasing/purchasing.yaml"), "--quiet"]);
    assert_eq!(code(&out), 0);
    assert!(out.stdout.is_empty(), "quiet success prints nothing");
}

#[test]
fn test_quiet_still_reports_failures() {
    let dir = scratch("quiet-fail");
    let policy = dir.join("fail.yaml");
    std::fs::write(
        &policy,
        "policy: t\nrules:\n  - id: a\n    when:\n      x: { equals: 1 }\n    then:\n      decision: approved\ntests:\n  - name: wrong\n    input: { x: 2 }\n    expect: { decision: approved }\n",
    )
    .unwrap();
    let out = run(&["test", policy.to_str().unwrap(), "--quiet"]);
    assert_eq!(code(&out), 4);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("FAIL wrong"),
        "failure is printed: {stdout}"
    );
    assert!(stdout.contains("1 of 1 tests failed"));
}

#[test]
fn validate_quiet_is_silent_when_valid_and_loud_when_not() {
    let ok = run(&["validate", &example("expense/expense.yaml"), "--quiet"]);
    assert_eq!(code(&ok), 0);
    assert!(ok.stdout.is_empty());

    let bad = run(&[
        "validate",
        &example("invalid/unknown-operator.yaml"),
        "--quiet",
    ]);
    assert_eq!(code(&bad), 2);
    assert!(!bad.stderr.is_empty(), "errors still print when quiet");
}

#[test]
fn oversized_input_file_is_refused() {
    let dir = scratch("big");
    let big: PathBuf = dir.join("big.json");
    // Just over the 10 MB limit. Content is valid JSON so only the size check can fail.
    let padding = "a".repeat(10 * 1024 * 1024);
    std::fs::write(&big, format!("{{\"pad\":\"{padding}\"}}")).unwrap();
    let out = run(&[
        "check",
        &example("expense/expense.yaml"),
        big.to_str().unwrap(),
    ]);
    assert_eq!(code(&out), 3);
    assert!(String::from_utf8_lossy(&out.stderr).contains("larger than 10 MB"));
}

#[test]
fn input_fingerprint_ignores_key_order_and_whitespace() {
    let dir = scratch("fingerprint");
    let a = dir.join("a.json");
    let b = dir.join("b.json");
    std::fs::write(&a, r#"{"expense":{"amount":6200,"currency":"NZD"}}"#).unwrap();
    std::fs::write(
        &b,
        "{\n  \"expense\": {\n    \"currency\": \"NZD\",\n    \"amount\": 6200\n  }\n}",
    )
    .unwrap();
    let policy = example("expense/expense.yaml");
    let hash = |p: &Path| {
        let out = run(&["check", &policy, p.to_str().unwrap()]);
        let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        v["input_sha256"].as_str().unwrap().to_string()
    };
    let ha = hash(&a);
    assert_eq!(ha.len(), 64, "SHA-256 hex digest");
    assert_eq!(ha, hash(&b));
}
