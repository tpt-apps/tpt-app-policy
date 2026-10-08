//! `tpt-policy`: command-line front end for TPT App Policy.

use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use serde_json::Value;
use tpt_commercial_cli::exit;
use tpt_policy_core::{evaluate, parse_policy, run_tests, Decision, Evaluation, Policy};

#[derive(Parser)]
#[command(
    name = "tpt-policy",
    version,
    about = "Evaluate deterministic business rules against JSON input"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Check a policy file for errors without evaluating anything
    Validate { policy: PathBuf },
    /// Evaluate a policy against an input file and print the decision
    Check {
        policy: PathBuf,
        /// JSON input file, or - to read stdin
        input: PathBuf,
        #[arg(long, value_enum, default_value = "json")]
        format: Format,
    },
    /// Like check, but also lists failed rules and why they failed
    Explain {
        policy: PathBuf,
        /// JSON input file, or - to read stdin
        input: PathBuf,
        #[arg(long, value_enum, default_value = "text")]
        format: Format,
    },
    /// Read JSON from stdin and print the decision as JSON
    Run { policy: PathBuf },
    /// Run the inline tests defined in the policy file
    Test { policy: PathBuf },
}

#[derive(Clone, Copy, ValueEnum)]
enum Format {
    Json,
    Text,
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Validate { policy } => match load_policy(&policy) {
            Ok(p) => {
                println!(
                    "valid: {} (version {}, {} rules, {} tests)",
                    p.name,
                    p.version,
                    p.rules.len(),
                    p.tests.len()
                );
                ExitCode::from(exit::SUCCESS)
            }
            Err(code) => code,
        },
        Command::Check {
            policy,
            input,
            format,
        } => decide(&policy, &input, format, false),
        Command::Explain {
            policy,
            input,
            format,
        } => decide(&policy, &input, format, true),
        Command::Run { policy } => decide(&policy, Path::new("-"), Format::Json, false),
        Command::Test { policy } => run_policy_tests(&policy),
    }
}

fn load_policy(path: &Path) -> Result<Policy, ExitCode> {
    let text = fs::read_to_string(path).map_err(|e| {
        eprintln!("error: cannot read policy '{}': {e}", path.display());
        ExitCode::from(exit::IO_ERROR)
    })?;
    parse_policy(&text).map_err(|e| {
        eprintln!("{e}\n  file: {}", path.display());
        ExitCode::from(exit::INVALID_POLICY)
    })
}

fn read_input(path: &Path) -> Result<Value, ExitCode> {
    let (source, text) = if path == Path::new("-") {
        let mut buf = String::new();
        let result = io::stdin().read_to_string(&mut buf);
        ("stdin".to_string(), result.map(|_| buf))
    } else {
        (path.display().to_string(), fs::read_to_string(path))
    };
    let text = text.map_err(|e| {
        eprintln!("error: cannot read input '{source}': {e}");
        ExitCode::from(exit::IO_ERROR)
    })?;
    serde_json::from_str(&text).map_err(|e| {
        eprintln!(
            "error: input is not valid JSON\n  why: {e}\n  fix: check the JSON syntax at the line shown\n  file: {source}"
        );
        ExitCode::from(exit::INVALID_INPUT)
    })
}

fn decide(policy_path: &Path, input_path: &Path, format: Format, explain: bool) -> ExitCode {
    let policy = match load_policy(policy_path) {
        Ok(p) => p,
        Err(code) => return code,
    };
    let input = match read_input(input_path) {
        Ok(v) => v,
        Err(code) => return code,
    };

    let evaluation = evaluate(&policy, &input);
    match format {
        Format::Json => println!(
            "{}",
            serde_json::to_string_pretty(&evaluation).expect("evaluation is always serialisable")
        ),
        Format::Text => print!("{}", render_text(&evaluation, explain)),
    }
    ExitCode::from(decision_exit_code(evaluation.decision))
}

fn run_policy_tests(path: &Path) -> ExitCode {
    let policy = match load_policy(path) {
        Ok(p) => p,
        Err(code) => return code,
    };
    if policy.tests.is_empty() {
        println!("no tests defined in {}", policy.name);
        return ExitCode::from(exit::SUCCESS);
    }

    let outcomes = run_tests(&policy);
    let mut failed = 0;
    for outcome in &outcomes {
        if outcome.passed {
            println!("PASS {}", outcome.name);
        } else {
            failed += 1;
            println!("FAIL {}: {}", outcome.name, outcome.message);
        }
    }
    println!();
    if failed == 0 {
        println!("{} tests passed", outcomes.len());
        ExitCode::from(exit::SUCCESS)
    } else {
        println!("{failed} of {} tests failed", outcomes.len());
        ExitCode::from(exit::TEST_FAILED)
    }
}

fn decision_exit_code(decision: Decision) -> u8 {
    match decision {
        Decision::Approved => exit::APPROVED,
        Decision::Review => exit::REVIEW,
        Decision::ApprovalRequired => exit::APPROVAL_REQUIRED,
        Decision::Rejected => exit::REJECTED,
    }
}

fn render_text(ev: &Evaluation, explain: bool) -> String {
    let mut lines = vec![
        format!("decision: {}", ev.decision),
        format!("policy: {} (version {})", ev.policy.name, ev.policy.version),
    ];
    if !ev.approvers.is_empty() {
        lines.push(format!("approvers: {}", ev.approvers.join(", ")));
    }
    if !ev.requirements.is_empty() {
        lines.push(format!("requirements: {}", ev.requirements.join(", ")));
    }
    if ev.matched_rules.is_empty() {
        lines.push("matched rules: none (default decision applied)".to_string());
    } else {
        lines.push(format!("matched rules: {}", ev.matched_rules.join(", ")));
    }
    for warning in &ev.warnings {
        lines.push(format!("warning: {warning}"));
    }
    if explain {
        lines.push(String::new());
        lines.push("explanations:".to_string());
        for e in &ev.explanations {
            lines.push(format!("  {} ({}): {}", e.rule, e.decision, e.message));
        }
        lines.push("failed rules:".to_string());
        for f in &ev.failed_rules {
            lines.push(format!("  {}: {}", f.rule, f.reason));
        }
    }
    lines.push(String::new());
    lines.join("\n")
}
