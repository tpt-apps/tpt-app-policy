//! `tpt-policy`: command-line front end for TPT App Policy.

use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use clap::{Parser, Subcommand, ValueEnum};
use serde_json::{json, Value};
use tpt_commercial_cli::config;
use tpt_commercial_cli::exit;
use tpt_commercial_cli::log::{LogOptions, Logger};
use tpt_policy_core::{evaluate, parse_policy, run_tests, Decision, Policy};

mod doctor;
mod serve;

const AFTER_HELP: &str = "\
Quick start:
  tpt-policy validate policies/expense.yaml
  tpt-policy check policies/expense.yaml examples/expense/expense.json
  tpt-policy explain policies/expense.yaml examples/expense/expense.json
  tpt-policy test policies/purchasing.yaml

A bare policy name such as 'expense' is also looked up in ./tpt/policies/.

Exit codes (not every command uses every code):
  0   Success. For a decision: approved
  1   A file could not be read or written
  2   A policy, schema or pipeline file is invalid, or the command line is wrong
  3   The input is not valid JSON or is too large
  4   One or more inline tests failed
  5   doctor found a failed check
  10  Decision: approval required
  20  Decision: review
  30  Decision: rejected

Logs (stderr only, never input values): --verbose, --debug, --json-logs.
Every command has its own --help, with examples.";
#[derive(Parser)]
#[command(
    name = "tpt-policy",
    version,
    about = "Evaluate deterministic business rules against JSON input",
    after_help = AFTER_HELP
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
    #[command(flatten)]
    log: LogOptions,
}

#[derive(Subcommand)]
enum Command {
    /// Check a policy file for errors without evaluating anything
    #[command(
        after_help = "Example:\n  tpt-policy validate policies/expense.yaml\n\nPrints 'valid: <name>@<version>' when the policy has no errors. Errors give the line, what is wrong, and how to fix it."
    )]
    Validate {
        /// The policy file (YAML), or a bare name looked up in ./tpt/policies/
        policy: PathBuf,
        /// Print nothing when the policy is valid. Errors still print.
        #[arg(long)]
        quiet: bool,
    },
    /// Evaluate a policy against an input file and print the decision
    #[command(
        after_help = "Examples:\n  tpt-policy check policies/expense.yaml examples/expense/expense.json\n  tpt-policy check policies/expense.yaml input.json --format html --output decision.html\n  tpt-policy check policies/expense.yaml - < input.json\n\nThe output is JSON by default: decision, approvers, requirements, matched_rules, failed_rules, input_sha256 and versions. The same input always gives the same bytes."
    )]
    Check {
        /// The policy file (YAML), or a bare name looked up in ./tpt/policies/
        policy: PathBuf,
        /// The input record as JSON. Use - to read it from stdin
        input: PathBuf,
        /// json: machine-readable. text: for a terminal. html: one page for people to read and file
        #[arg(long, value_enum, default_value = "json")]
        format: Format,
        /// Write the result to this file instead of stdout
        #[arg(long, value_name = "FILE")]
        output: Option<PathBuf>,
    },
    /// Like check, but also lists failed rules and why they failed
    #[command(
        after_help = "Example:\n  tpt-policy explain policies/expense.yaml examples/expense/expense.json\n\nUse this when a decision surprises someone: it shows every rule that did not match and the field that caused it."
    )]
    Explain {
        /// The policy file (YAML), or a bare name looked up in ./tpt/policies/
        policy: PathBuf,
        /// The input record as JSON. Use - to read it from stdin
        input: PathBuf,
        /// text: for a terminal (the default). json: machine-readable. html: one page
        #[arg(long, value_enum, default_value = "text")]
        format: Format,
        /// Write the result to this file instead of stdout
        #[arg(long, value_name = "FILE")]
        output: Option<PathBuf>,
    },
    /// Read JSON from stdin and print the decision as JSON
    #[command(
        after_help = "Example:\n  cat examples/expense/expense.json | tpt-policy run policies/expense.yaml\n\nFor scripts and pipelines. Reads one JSON object from stdin and prints the JSON decision."
    )]
    Run {
        /// The policy file (YAML), or a bare name looked up in ./tpt/policies/
        policy: PathBuf,
        /// Write the result to this file instead of stdout
        #[arg(long, value_name = "FILE")]
        output: Option<PathBuf>,
    },
    /// Run the inline tests defined in the policy file
    #[command(
        after_help = "Example:\n  tpt-policy test policies/purchasing.yaml\n\nEach test in the policy's 'tests:' block gives an input and the decision it must produce. Exits 4 if any test fails."
    )]
    Test {
        /// The policy file (YAML), or a bare name looked up in ./tpt/policies/
        policy: PathBuf,
        /// Print only failing tests and the summary of failures
        #[arg(long)]
        quiet: bool,
    },
    /// Serve evaluations over HTTP (POST /v1/evaluate)
    #[command(
        after_help = "Examples:\n  tpt-policy serve policies/expense.yaml\n  TPT_POLICY_TOKEN=change-me tpt-policy serve policies/expense.yaml --listen 0.0.0.0:8080\n\nListens on 127.0.0.1:8080 by default. Requests are limited to 1 MB."
    )]
    Serve {
        /// The policy file (YAML), or a bare name looked up in ./tpt/policies/
        policy: PathBuf,
        /// Address and port to listen on. The default only accepts connections from this machine
        #[arg(long, default_value = "127.0.0.1:8080", value_name = "ADDR")]
        listen: String,
        /// Bearer token that callers must send. Set TPT_POLICY_TOKEN instead of passing it here, so it stays out of shell history
        #[arg(long, env = "TPT_POLICY_TOKEN", hide_env_values = true)]
        token: Option<String>,
    },
    /// Check that this install works: version, platform, folders and a self-test
    #[command(
        after_help = "Example:\n  tpt-policy doctor\n\nRun this first after installing. It exits 5 if a required check fails."
    )]
    Doctor,
}

#[derive(Clone, Copy, ValueEnum)]
enum Format {
    Json,
    Text,
    /// A single HTML page for people to read and file
    Html,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let log = Logger::new(cli.log);
    match cli.command {
        Command::Validate { policy, quiet } => match load_policy(&policy, &log) {
            Ok(p) => {
                if !quiet {
                    println!(
                        "valid: {}@{} ({} rules, {} tests)",
                        p.name,
                        p.version,
                        p.rules.len(),
                        p.tests.len()
                    );
                }
                ExitCode::from(exit::SUCCESS)
            }
            Err(code) => code,
        },
        Command::Check {
            policy,
            input,
            format,
            output,
        } => decide(&policy, &input, format, false, output.as_deref(), &log),
        Command::Explain {
            policy,
            input,
            format,
            output,
        } => decide(&policy, &input, format, true, output.as_deref(), &log),
        Command::Run { policy, output } => decide(
            &policy,
            Path::new("-"),
            Format::Json,
            false,
            output.as_deref(),
            &log,
        ),
        Command::Test { policy, quiet } => run_policy_tests(&policy, quiet, &log),
        Command::Serve {
            policy,
            listen,
            token,
        } => serve_policy(&policy, &listen, token, &log),
        Command::Doctor => ExitCode::from(doctor::run()),
    }
}

fn serve_policy(path: &Path, listen: &str, token: Option<String>, log: &Logger) -> ExitCode {
    let policy = match load_policy(path, log) {
        Ok(p) => p,
        Err(code) => return code,
    };
    if token.is_none() && !is_loopback(listen) {
        eprintln!(
            "warning: listening on {listen} without a token; anyone who can reach this address can evaluate policies. Set TPT_POLICY_TOKEN or listen on 127.0.0.1."
        );
    }
    log.info(
        "serving",
        &[("listen", json!(listen)), ("auth", json!(token.is_some()))],
    );
    match serve::serve(policy, listen, token) {
        Ok(()) => ExitCode::from(exit::SUCCESS),
        Err(e) => {
            eprintln!("error: cannot serve on {listen}: {e}\n  fix: check the address is valid and the port is free");
            ExitCode::from(exit::IO_ERROR)
        }
    }
}

fn is_loopback(listen: &str) -> bool {
    let host = listen.rsplit_once(':').map_or(listen, |(host, _)| host);
    matches!(host, "127.0.0.1" | "localhost" | "[::1]" | "::1")
}

fn load_policy(arg: &Path, log: &Logger) -> Result<Policy, ExitCode> {
    let path = config::resolve_policy(arg);
    if path != arg {
        log.debug(
            "policy found in config folder",
            &[
                ("from", json!(arg.display().to_string())),
                ("path", json!(path.display().to_string())),
            ],
        );
    }
    let text = fs::read_to_string(&path).map_err(|e| {
        eprintln!("error: cannot read policy '{}': {e}", path.display());
        ExitCode::from(exit::IO_ERROR)
    })?;
    let policy = parse_policy(&text).map_err(|e| {
        eprintln!("{e}\n  file: {}", path.display());
        ExitCode::from(exit::INVALID_POLICY)
    })?;
    log.info(
        "policy loaded",
        &[
            ("policy", json!(policy.name)),
            ("version", json!(policy.version)),
            ("rules", json!(policy.rules.len())),
            ("path", json!(path.display().to_string())),
        ],
    );
    Ok(policy)
}

/// Largest input the CLI will read. Larger inputs are refused, so one command
/// cannot exhaust memory. The HTTP service has its own 1 MB limit.
const MAX_INPUT_BYTES: u64 = 10 * 1024 * 1024;

fn read_input(path: &Path, log: &Logger) -> Result<Value, ExitCode> {
    let (source, text) = if path == Path::new("-") {
        let mut buf = String::new();
        let result = io::stdin()
            .take(MAX_INPUT_BYTES + 1)
            .read_to_string(&mut buf);
        ("stdin".to_string(), result.map(|_| buf))
    } else {
        let too_big = fs::metadata(path).is_ok_and(|m| m.len() > MAX_INPUT_BYTES);
        if too_big {
            eprintln!(
                "error: input is larger than {} MB\n  why: the CLI reads inputs up to this size\n  fix: split the input into smaller files, or use the HTTP service for single records\n  file: {}",
                MAX_INPUT_BYTES / (1024 * 1024),
                path.display()
            );
            return Err(ExitCode::from(exit::INVALID_INPUT));
        }
        (path.display().to_string(), fs::read_to_string(path))
    };
    let text = text.map_err(|e| {
        eprintln!("error: cannot read input '{source}': {e}");
        ExitCode::from(exit::IO_ERROR)
    })?;
    if text.len() as u64 > MAX_INPUT_BYTES {
        eprintln!(
            "error: input is larger than {} MB\n  why: the CLI reads inputs up to this size\n  fix: split the input into smaller files, or use the HTTP service for single records\n  file: {source}",
            MAX_INPUT_BYTES / (1024 * 1024)
        );
        return Err(ExitCode::from(exit::INVALID_INPUT));
    }
    log.info(
        "input read",
        &[("source", json!(source)), ("bytes", json!(text.len()))],
    );
    serde_json::from_str(&text).map_err(|e| {
        eprintln!(
            "error: input is not valid JSON\n  why: {e}\n  fix: check the JSON syntax at the line shown\n  file: {source}"
        );
        ExitCode::from(exit::INVALID_INPUT)
    })
}

fn decide(
    policy_path: &Path,
    input_path: &Path,
    format: Format,
    explain: bool,
    output: Option<&Path>,
    log: &Logger,
) -> ExitCode {
    let started = Instant::now();
    let policy = match load_policy(policy_path, log) {
        Ok(p) => p,
        Err(code) => return code,
    };
    let input = match read_input(input_path, log) {
        Ok(v) => v,
        Err(code) => return code,
    };

    let evaluation = evaluate(&policy, &input);
    let code = decision_exit_code(evaluation.decision);
    log.info(
        "decision",
        &[
            ("decision", json!(evaluation.decision.to_string())),
            ("exit_code", json!(code)),
            ("elapsed_ms", json!(started.elapsed().as_millis() as u64)),
        ],
    );
    log.debug(
        "evaluation details",
        &[
            ("input_sha256", json!(evaluation.input_sha256)),
            ("matched_rules", json!(evaluation.matched_rules)),
            ("failed_rules", json!(evaluation.failed_rules.len())),
            ("engine_version", json!(evaluation.engine_version)),
        ],
    );
    let text = match format {
        Format::Json => format!("{}\n", tpt_report::json(&evaluation)),
        Format::Text => tpt_report::terminal(&evaluation, explain),
        Format::Html => tpt_report::evaluation_html(&evaluation),
    };
    if let Some(path) = output {
        if let Err(e) = fs::write(path, text) {
            eprintln!(
                "error: cannot write output '{}': {e}\n  fix: check the folder exists and is writable",
                path.display()
            );
            return ExitCode::from(exit::IO_ERROR);
        }
    } else {
        print!("{text}");
    }
    ExitCode::from(code)
}

fn run_policy_tests(path: &Path, quiet: bool, log: &Logger) -> ExitCode {
    let policy = match load_policy(path, log) {
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
            if !quiet {
                println!("PASS {}", outcome.name);
            }
        } else {
            failed += 1;
            println!("FAIL {}: {}", outcome.name, outcome.message);
        }
    }
    log.info(
        "tests run",
        &[("total", json!(outcomes.len())), ("failed", json!(failed))],
    );
    if failed == 0 {
        if !quiet {
            println!();
            println!("{} tests passed", outcomes.len());
        }
        ExitCode::from(exit::SUCCESS)
    } else {
        println!();
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
