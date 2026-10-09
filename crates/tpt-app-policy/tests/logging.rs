//! Tests for --verbose, --debug, --json-logs (spec §38) and config discovery
//! (spec §19). Logs go to stderr, so every test checks stdout as well.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn example(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(relative)
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tpt-logging-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tpt-policy"))
        .args(args)
        .output()
        .expect("tpt-policy runs")
}

fn run_in(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tpt-policy"))
        .current_dir(dir)
        .args(args)
        .output()
        .expect("tpt-policy runs")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn check_args(policy: &str, input: &str) -> Vec<String> {
    vec!["check".to_string(), policy.to_string(), input.to_string()]
}

fn with_flags(flags: &[&str], base: &[String]) -> Vec<String> {
    let mut args: Vec<String> = base.to_vec();
    args.extend(flags.iter().map(|f| f.to_string()));
    args
}

fn as_strs(args: &[String]) -> Vec<&str> {
    args.iter().map(String::as_str).collect()
}

fn expense_args() -> Vec<String> {
    check_args(
        &example("expense/expense.yaml").to_string_lossy(),
        &example("expense/expense.json").to_string_lossy(),
    )
}

#[test]
fn logs_are_off_by_default() {
    let out = run(&as_strs(&expense_args()));
    assert!(stderr(&out).is_empty(), "stderr: {}", stderr(&out));
}

#[test]
fn verbose_logs_steps_to_stderr_and_leaves_stdout_alone() {
    let plain = run(&as_strs(&expense_args()));
    let verbose = run(&as_strs(&with_flags(&["--verbose"], &expense_args())));
    assert_eq!(plain.stdout, verbose.stdout, "stdout must not change");
    let text = stderr(&verbose);
    assert!(text.contains("info: policy loaded"), "stderr: {text}");
    assert!(text.contains("info: input read"), "stderr: {text}");
    assert!(text.contains("info: decision"), "stderr: {text}");
    assert!(
        !text.contains("debug:"),
        "verbose must not print debug lines"
    );
}

#[test]
fn json_logs_are_one_object_per_line() {
    let out = run(&as_strs(&with_flags(
        &["--verbose", "--json-logs"],
        &expense_args(),
    )));
    let text = stderr(&out);
    let lines: Vec<&str> = text.lines().collect();
    assert!(!lines.is_empty(), "expected log lines");
    for line in lines {
        let value: serde_json::Value =
            serde_json::from_str(line).unwrap_or_else(|e| panic!("not JSON ({e}): {line}"));
        assert_eq!(value["level"], "info", "line: {line}");
        assert!(value["message"].is_string(), "line: {line}");
        assert!(value["fields"].is_object(), "line: {line}");
    }
}

#[test]
fn debug_adds_detail_but_never_input_values() {
    // The secret sits in a field the policy does not read, so it can only
    // reach the logs if the logger echoes the input.
    let dir = scratch("secret");
    let input = dir.join("input.json");
    let mut value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(example("expense/expense.json")).unwrap())
            .unwrap();
    value["customer_note"] = serde_json::Value::from("CUSTOMER-SECRET-7731");
    std::fs::write(&input, value.to_string()).unwrap();

    let args = check_args(
        &example("expense/expense.yaml").to_string_lossy(),
        &input.to_string_lossy(),
    );
    let out = run(&as_strs(&with_flags(&["--debug"], &args)));
    let text = stderr(&out);
    assert!(text.contains("debug: evaluation details"), "stderr: {text}");
    assert!(text.contains("input_sha256="), "stderr: {text}");
    assert!(
        !text.contains("CUSTOMER-SECRET-7731"),
        "input leaked: {text}"
    );
}

#[test]
fn debug_json_logs_carry_the_same_detail() {
    let out = run(&as_strs(&with_flags(
        &["--debug", "--json-logs"],
        &expense_args(),
    )));
    let text = stderr(&out);
    let debug_line = text
        .lines()
        .find(|l| l.contains("\"evaluation details\""))
        .unwrap_or_else(|| panic!("no debug line in: {text}"));
    let value: serde_json::Value = serde_json::from_str(debug_line).unwrap();
    assert_eq!(value["level"], "debug");
    assert!(value["fields"]["input_sha256"].is_string());
    assert!(value["fields"]["matched_rules"].is_array());
}

#[test]
fn bare_policy_name_resolves_from_config_folder() {
    let dir = scratch("discover");
    let policies = dir.join("tpt").join("policies");
    std::fs::create_dir_all(&policies).unwrap();
    std::fs::copy(
        example("expense/expense.yaml"),
        policies.join("expense.yaml"),
    )
    .unwrap();

    let input = example("expense/expense.json");
    let found = run_in(&dir, &["check", "expense", &input.to_string_lossy()]);
    let direct = run(&as_strs(&expense_args()));
    assert_eq!(found.status.code(), direct.status.code());
    assert_eq!(found.stdout, direct.stdout, "same policy, same decision");
}

#[test]
fn bare_policy_name_not_found_names_what_was_typed() {
    let dir = scratch("missing");
    let input = example("expense/expense.json");
    let out = run_in(&dir, &["check", "nosuchpolicy", &input.to_string_lossy()]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).contains("nosuchpolicy"),
        "stderr: {}",
        stderr(&out)
    );
}

#[test]
fn verbose_says_where_the_policy_came_from() {
    let dir = scratch("discover-debug");
    let policies = dir.join("tpt").join("policies");
    std::fs::create_dir_all(&policies).unwrap();
    std::fs::copy(
        example("expense/expense.yaml"),
        policies.join("expense.yaml"),
    )
    .unwrap();

    let input = example("expense/expense.json");
    let out = run_in(
        &dir,
        &["--debug", "check", "expense", &input.to_string_lossy()],
    );
    assert!(
        stderr(&out).contains("debug: policy found in config folder"),
        "stderr: {}",
        stderr(&out)
    );
}
