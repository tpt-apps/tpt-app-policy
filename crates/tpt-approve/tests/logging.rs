//! The shared logging flags (spec §38) on this binary. `doctor` runs without
//! input files, so it is a safe command to log.

use std::process::Command;

fn run(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_tpt-approve"))
        .args(args)
        .output()
        .expect("tpt-approve runs")
}

#[test]
fn logs_are_off_by_default() {
    let out = run(&["doctor"]);
    assert!(
        out.stderr.is_empty(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn json_logs_report_start_and_exit_code() {
    let out = run(&["--verbose", "--json-logs", "doctor"]);
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    let code = out.status.code().expect("exits normally");
    assert!(
        stderr.contains("\"message\":\"command started\""),
        "stderr: {stderr}"
    );
    let finished = stderr
        .lines()
        .rev()
        .find(|l| l.contains("\"message\":\"command finished\""))
        .unwrap_or_else(|| panic!("no finish line in: {stderr}"));
    assert!(
        finished.contains(&format!("\"exit_code\":{code}")),
        "line: {finished}"
    );
    for line in stderr.lines() {
        let value: serde_json::Value =
            serde_json::from_str(line).unwrap_or_else(|e| panic!("not JSON ({e}): {line}"));
        assert!(value["level"].is_string(), "line: {line}");
    }
}
