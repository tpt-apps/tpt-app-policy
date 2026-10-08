//! `tpt-policy doctor`: checks that the install works (spec §29.5).
//!
//! Checks: version, platform, a writable working directory, and a built-in
//! self-test that parses and evaluates a small policy. No optional
//! dependencies are needed yet, so none are checked.

use std::env;
use std::fs;
use std::path::Path;

use tpt_commercial_cli::exit;
use tpt_policy_core::{evaluate, parse_policy, Decision, ENGINE_VERSION};

const SELF_TEST_POLICY: &str = "\
policy: doctor-self-test
rules:
  - id: over-limit
    when:
      amount:
        gt: 100
    then:
      decision: approval_required
";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Status {
    Ok,
    Warn,
    Fail,
}

impl Status {
    fn label(self) -> &'static str {
        match self {
            Status::Ok => "ok",
            Status::Warn => "warn",
            Status::Fail => "fail",
        }
    }
}

/// Run every check, print the results, and return the exit code.
pub fn run() -> u8 {
    let checks = [
        check_version(),
        check_platform(),
        check_working_directory(),
        check_config_directory(),
        check_self_test(),
    ];

    for (status, label, detail) in &checks {
        println!("[{}] {label}: {detail}", status.label());
    }

    if checks.iter().any(|(s, _, _)| *s == Status::Fail) {
        println!("\nsome checks failed; see the fix hints above");
        exit::DOCTOR_FAILED
    } else {
        println!("\nall required checks passed");
        exit::SUCCESS
    }
}

type Check = (Status, &'static str, String);

fn check_version() -> Check {
    (
        Status::Ok,
        "version",
        format!(
            "tpt-policy {}, engine {ENGINE_VERSION}",
            env!("CARGO_PKG_VERSION")
        ),
    )
}

fn check_platform() -> Check {
    let (os, arch) = (env::consts::OS, env::consts::ARCH);
    if os == "windows" && arch == "x86_64" || os == "linux" && arch == "x86_64" {
        (Status::Ok, "platform", format!("{os} {arch}"))
    } else {
        (
            Status::Warn,
            "platform",
            format!(
                "{os} {arch}; the supported targets are Windows x64 and Linux x64, so this build is not covered by support"
            ),
        )
    }
}

fn check_working_directory() -> Check {
    let dir = match env::current_dir() {
        Ok(d) => d,
        Err(e) => {
            return (
                Status::Fail,
                "working directory",
                format!(
                "cannot read the current directory: {e}. Fix: run from a directory you can access"
            ),
            )
        }
    };
    let probe = dir.join(format!(".tpt-doctor-{}.tmp", std::process::id()));
    match fs::write(&probe, b"ok").and_then(|_| fs::remove_file(&probe)) {
        Ok(()) => (
            Status::Ok,
            "working directory",
            format!("{} is writable", dir.display()),
        ),
        Err(e) => (
            Status::Fail,
            "working directory",
            format!(
                "{} is not writable: {e}. Fix: run from a directory you can write to",
                dir.display()
            ),
        ),
    }
}

/// The config layout from spec §19. The directory itself is optional.
fn check_config_directory() -> Check {
    const SUBDIRS: [&str; 5] = ["config", "policies", "schemas", "templates", "reports"];
    let root = Path::new("tpt");
    if !root.is_dir() {
        return (
            Status::Ok,
            "config directory",
            "./tpt not found; this is optional, so built-in defaults apply".to_string(),
        );
    }
    let missing: Vec<&str> = SUBDIRS
        .into_iter()
        .filter(|d| !root.join(d).is_dir())
        .collect();
    if missing.is_empty() {
        (
            Status::Ok,
            "config directory",
            "./tpt has config, policies, schemas, templates and reports".to_string(),
        )
    } else {
        (
            Status::Warn,
            "config directory",
            format!(
                "./tpt is missing {}. Fix: create those folders, or delete ./tpt to use defaults",
                missing.join(", ")
            ),
        )
    }
}

fn check_self_test() -> Check {
    let result = parse_policy(SELF_TEST_POLICY)
        .map(|policy| evaluate(&policy, &serde_json::json!({"amount": 500})).decision);
    match result {
        Ok(Decision::ApprovalRequired) => (
            Status::Ok,
            "self-test",
            "built-in policy parsed and evaluated correctly".to_string(),
        ),
        Ok(other) => (
            Status::Fail,
            "self-test",
            format!("built-in policy returned {other}, expected approval_required. Fix: reinstall tpt-policy"),
        ),
        Err(e) => (
            Status::Fail,
            "self-test",
            format!("built-in policy failed to parse: {e}. Fix: reinstall tpt-policy"),
        ),
    }
}
