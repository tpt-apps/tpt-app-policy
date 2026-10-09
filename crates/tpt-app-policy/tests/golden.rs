//! Golden tests: the CLI's JSON output must match the checked-in files exactly.

use std::path::PathBuf;
use std::process::Command;

fn example(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("examples")
        .join(path)
}

#[test]
fn expense_check_matches_golden_output() {
    let output = Command::new(env!("CARGO_BIN_EXE_tpt-policy"))
        .arg("check")
        .arg(example("expense/expense.yaml"))
        .arg(example("expense/expense.json"))
        .output()
        .expect("tpt-policy runs");

    let expected = std::fs::read_to_string(example("expense/expense.expected.json"))
        .expect("golden file exists");
    let actual = String::from_utf8(output.stdout).expect("stdout is UTF-8");

    assert_eq!(actual.trim_end(), expected.trim_end());
    assert_eq!(
        output.status.code(),
        Some(10),
        "approval_required exit code"
    );
}

#[test]
fn purchasing_inline_tests_pass() {
    let output = Command::new(env!("CARGO_BIN_EXE_tpt-policy"))
        .arg("test")
        .arg(example("purchasing/purchasing.yaml"))
        .output()
        .expect("tpt-policy runs");

    assert_eq!(output.status.code(), Some(0));
    let stdout = String::from_utf8(output.stdout).expect("stdout is UTF-8");
    assert!(
        stdout.contains("tests passed"),
        "unexpected output: {stdout}"
    );
}
