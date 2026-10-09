//! `tpt-approve`: command-line front end for TPT Approval Engine.
//!
//! Exit codes are the shared ones from `tpt-commercial-cli`:
//!
//! | Code | Meaning |
//! |---|---|
//! | 0 | Approved automatically (or the rules are valid) |
//! | 1 | A file could not be read or written |
//! | 2 | Invalid approval rules, or a bad command line |
//! | 3 | The request is not a valid JSON object |
//! | 5 | `doctor` found a problem |
//! | 10 | Approval is required |
//! | 20 | Review: no rule covers the request |

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use serde_json::{json, Value};
use tpt_approve::{analyse, check, parse_approval, ApprovalFile, APPROVE_VERSION};
use tpt_commercial_cli::exit;
use tpt_commercial_cli::log::{LogOptions, Logger};
use tpt_policy_core::Decision;

#[derive(Parser)]
#[command(
    name = "tpt-approve",
    version,
    about = "Say who must approve a request, from deterministic approval rules"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
    #[command(flatten)]
    log: LogOptions,
}

#[derive(Subcommand)]
enum Command {
    /// Check an approval rules file for errors, and for gaps in the amounts it covers
    Validate {
        /// Approval rules file (YAML)
        #[arg(long, value_name = "FILE")]
        rules: PathBuf,
    },
    /// Say who must approve one request (a JSON object)
    Check {
        /// The request, as a JSON file
        request: PathBuf,
        /// Approval rules file (YAML)
        #[arg(long, value_name = "FILE")]
        rules: PathBuf,
        /// Output format
        #[arg(long, value_enum, default_value_t = Format::Json)]
        format: Format,
    },
    /// Check that this install works
    Doctor,
}

#[derive(Clone, Copy, ValueEnum)]
enum Format {
    Json,
    Text,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let log = Logger::new(cli.log);
    let started = log.started(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
    let code = match cli.command {
        Command::Validate { rules } => validate(&rules),
        Command::Check {
            request,
            rules,
            format,
        } => check_request(&request, &rules, format),
        Command::Doctor => doctor(),
    };
    log.finished(started, code);
    ExitCode::from(code)
}

fn load_rules(path: &Path) -> Result<ApprovalFile, u8> {
    let text = fs::read_to_string(path).map_err(|e| {
        eprintln!(
            "error: cannot read rules '{}': {e}\n  fix: check the path to the rules file",
            path.display()
        );
        exit::IO_ERROR
    })?;
    parse_approval(&text).map_err(|why| {
        eprintln!(
            "error: invalid approval rules '{}'\n  why: {why}\n  fix: check the rule and its indentation",
            path.display()
        );
        exit::INVALID_POLICY
    })
}

fn validate(rules: &Path) -> u8 {
    let file = match load_rules(rules) {
        Ok(file) => file,
        Err(code) => return code,
    };
    if let Err(why) = check(&file, &json!({})) {
        eprintln!("error: the rules do not compile\n  why: {why}");
        return exit::INVALID_POLICY;
    }
    println!(
        "rules ok: {} {} ({} rules)",
        file.approval,
        file.version,
        file.rules.len()
    );
    let analysis = analyse(&file);
    for note in &analysis.notes {
        println!("note: {note}");
    }
    for warning in &analysis.warnings {
        println!("warning: {warning}");
    }
    if analysis.warnings.is_empty() {
        println!("no gaps found in the tested fields");
    }
    exit::SUCCESS
}

fn check_request(request_path: &Path, rules: &Path, format: Format) -> u8 {
    let file = match load_rules(rules) {
        Ok(file) => file,
        Err(code) => return code,
    };
    let text = match fs::read_to_string(request_path) {
        Ok(text) => text,
        Err(e) => {
            eprintln!(
                "error: cannot read request '{}': {e}\n  fix: check the path to the request file",
                request_path.display()
            );
            return exit::IO_ERROR;
        }
    };
    let request: Value = match serde_json::from_str(&text) {
        Ok(value @ Value::Object(_)) => value,
        Ok(_) => {
            eprintln!(
                "error: the request '{}' is not a JSON object\n  fix: wrap the fields in {{ }} so the request is an object",
                request_path.display()
            );
            return exit::INVALID_INPUT;
        }
        Err(e) => {
            eprintln!(
                "error: the request '{}' is not valid JSON: {e}\n  fix: check the file for a missing comma or bracket",
                request_path.display()
            );
            return exit::INVALID_INPUT;
        }
    };
    let evaluation = match check(&file, &request) {
        Ok(evaluation) => evaluation,
        Err(why) => {
            eprintln!("error: the rules do not compile\n  why: {why}");
            return exit::INVALID_POLICY;
        }
    };
    match format {
        Format::Json => println!("{}", tpt_report::json(&evaluation)),
        Format::Text => print!("{}", tpt_report::terminal(&evaluation, true)),
    }
    decision_exit_code(evaluation.decision)
}

fn decision_exit_code(decision: Decision) -> u8 {
    match decision {
        Decision::Approved => exit::APPROVED,
        Decision::Review => exit::REVIEW,
        Decision::ApprovalRequired => exit::APPROVAL_REQUIRED,
        Decision::Rejected => exit::REJECTED,
    }
}

/// Rules used by the self-test. They cover three bands, so each band is checked.
const SELF_TEST_RULES: &str = "approval: doctor\nrules:\n  - when: {amount: \"<1000\"}\n    decision: automatic\n  - when: {amount: \"1000-5000\"}\n    approver: manager\n  - when: {amount: \">5000\"}\n    approver: director\n";

/// `tpt-approve doctor`: version, platform, a writable folder, and a self-test
/// that checks one request in each band of the built-in rules.
fn doctor() -> u8 {
    let mut failed = false;
    println!("[ok] version: tpt-approve {APPROVE_VERSION}");
    println!(
        "[ok] platform: {} {}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    let probe = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".tpt-approve-doctor-probe");
    match fs::write(&probe, b"ok").and_then(|_| fs::remove_file(&probe)) {
        Ok(()) => println!("[ok] working directory is writable"),
        Err(e) => {
            println!("[fail] working directory is not writable: {e}. Fix: run from a folder you can write to");
            failed = true;
        }
    }
    let self_test = parse_approval(SELF_TEST_RULES)
        .map_err(|e| e.to_string())
        .and_then(|file| {
            let small = check(&file, &json!({"amount": 500}))?;
            let mid = check(&file, &json!({"amount": 3000}))?;
            let large = check(&file, &json!({"amount": 9000}))?;
            let expected = small.decision == Decision::Approved
                && mid.approvers == ["manager"]
                && large.approvers == ["director"];
            if expected {
                Ok(())
            } else {
                Err("the built-in rules did not give the expected decisions; reinstall the release bundle".to_string())
            }
        });
    match self_test {
        Ok(()) => println!("[ok] self-test: built-in rules give the expected decisions"),
        Err(why) => {
            println!("[fail] self-test: {why}");
            failed = true;
        }
    }
    println!();
    if failed {
        println!("some checks failed");
        exit::DOCTOR_FAILED
    } else {
        println!("all checks passed");
        exit::SUCCESS
    }
}
