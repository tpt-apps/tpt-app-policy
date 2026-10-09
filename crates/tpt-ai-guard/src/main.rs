//! `tpt-ai-guard`: command-line front end for TPT AI Action Guard.
//!
//! The guard decides whether an action may go ahead. It never runs the action.
//!
//! Exit codes:
//!
//! | Code | Meaning |
//! |---|---|
//! | 0 | ALLOW |
//! | 1 | A file could not be read |
//! | 2 | Invalid action policy, or a bad command line |
//! | 3 | The request is not a valid JSON object with an `action` |
//! | 5 | `doctor` found a problem |
//! | 10 | REQUIRE_APPROVAL |
//! | 30 | DENY |

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use serde_json::Value;
use tpt_ai_guard::{
    compile, decide, decide_with_grants, parse_action_policy, parse_grants, ActionPolicy, Grants,
    GUARD_VERSION,
};
use tpt_commercial_cli::exit;
use tpt_commercial_cli::log::{LogOptions, Logger};

mod serve;

const AFTER_HELP: &str = "\
Quick start:
  tpt-ai-guard validate --policy examples/ai-guard/customer-actions.policy.yaml
  tpt-ai-guard check examples/ai-guard/requests/refund-large.json --policy examples/ai-guard/customer-actions.policy.yaml --grants examples/ai-guard/grants.yaml

Returns ALLOW, DENY or REQUIRE_APPROVAL for an action an AI agent asks to take. The most restrictive matching rule wins, and no match means DENY.

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
    name = "tpt-ai-guard",
    version,
    about = "Decide ALLOW, DENY or REQUIRE_APPROVAL for actions an AI agent asks to take",
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
    /// Decide one action request (a JSON object with an 'action' field)
    #[command(
        after_help = "Examples:\n  tpt-ai-guard check examples/ai-guard/requests/refund-large.json --policy examples/ai-guard/customer-actions.policy.yaml\n  tpt-ai-guard check request.json --policy policy.yaml --grants grants.yaml --format text\n\nExits 0 for ALLOW, 30 for DENY and 10 for REQUIRE_APPROVAL. Without --grants, resources named in the request are not checked."
    )]
    Check {
        /// The action request as a JSON object, with an 'action' field
        request: PathBuf,
        /// The action policy (YAML): the rules that decide each action
        #[arg(long, value_name = "FILE")]
        policy: PathBuf,
        /// Resources the agent may use (YAML: fs_read, net_connect, process_spawn). Without this, resources named in the request are not checked
        #[arg(long, value_name = "FILE")]
        grants: Option<PathBuf>,
        /// json: machine-readable (the default). text: for a terminal
        #[arg(long, value_enum, default_value_t = Format::Json)]
        format: Format,
    },
    /// Check an action policy file for errors
    #[command(
        after_help = "Example:\n  tpt-ai-guard validate --policy examples/ai-guard/customer-actions.policy.yaml\n\nChecks the file's structure and rules without deciding anything."
    )]
    Validate {
        /// The action policy (YAML) to check
        #[arg(long, value_name = "FILE")]
        policy: PathBuf,
    },
    /// Serve decisions over HTTP (POST /v1/decide)
    #[command(
        after_help = "Examples:\n  tpt-ai-guard serve --policy examples/ai-guard/customer-actions.policy.yaml\n  TPT_AI_GUARD_TOKEN=change-me tpt-ai-guard serve --policy policy.yaml --grants grants.yaml --listen 0.0.0.0:8080\n\nListens on 127.0.0.1:8080 by default. Requests are limited to 1 MB."
    )]
    Serve {
        /// The action policy (YAML)
        #[arg(long, value_name = "FILE")]
        policy: PathBuf,
        /// Resources the agent may use (YAML), as for 'check --grants'
        #[arg(long, value_name = "FILE")]
        grants: Option<PathBuf>,
        /// Address and port to listen on. The default only accepts connections from this machine
        #[arg(long, default_value = "127.0.0.1:8080", value_name = "ADDR")]
        listen: String,
        /// Bearer token that callers must send. Set TPT_AI_GUARD_TOKEN instead of passing it here, so it stays out of shell history
        #[arg(long, env = "TPT_AI_GUARD_TOKEN", hide_env_values = true)]
        token: Option<String>,
    },
    /// Check that this install works: version, platform, folders and a self-test
    #[command(
        after_help = "Example:\n  tpt-ai-guard doctor\n\nExits 5 if a required check fails."
    )]
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
        Command::Check {
            request,
            policy,
            grants,
            format,
        } => check(&request, &policy, grants.as_deref(), format),
        Command::Validate { policy } => validate(&policy),
        Command::Serve {
            policy,
            grants,
            listen,
            token,
        } => serve_guard(&policy, grants.as_deref(), &listen, token),
        Command::Doctor => doctor(),
    };
    log.finished(started, code);
    ExitCode::from(code)
}

fn load_policy(path: &Path) -> Result<ActionPolicy, u8> {
    let text = fs::read_to_string(path).map_err(|e| {
        eprintln!(
            "error: cannot read policy '{}': {e}\n  fix: check the path to the policy file",
            path.display()
        );
        exit::IO_ERROR
    })?;
    parse_action_policy(&text).map_err(|why| {
        eprintln!(
            "error: invalid action policy '{}'\n  why: {why}\n  fix: check the rule, its action and its decision",
            path.display()
        );
        exit::INVALID_POLICY
    })
}

fn serve_guard(
    policy_path: &Path,
    grants_path: Option<&Path>,
    listen: &str,
    token: Option<String>,
) -> u8 {
    let policy = match load_policy(policy_path) {
        Ok(policy) => policy,
        Err(code) => return code,
    };
    if let Err(why) = compile(&policy) {
        eprintln!("error: the policy does not compile\n  why: {why}");
        return exit::INVALID_POLICY;
    }
    let grants = match grants_path.map(load_grants).transpose() {
        Ok(grants) => grants,
        Err(code) => return code,
    };
    let is_loopback = {
        let host = listen.rsplit_once(':').map_or(listen, |(host, _)| host);
        matches!(host, "127.0.0.1" | "localhost" | "[::1]" | "::1")
    };
    if token.is_none() && !is_loopback {
        eprintln!(
            "warning: listening on {listen} without a token; anyone who can reach this address can ask for decisions. Set TPT_AI_GUARD_TOKEN or listen on 127.0.0.1."
        );
    }
    match serve::serve(policy, grants, listen, token) {
        Ok(()) => exit::SUCCESS,
        Err(e) => {
            eprintln!("error: cannot serve on {listen}: {e}\n  fix: check the address is valid and the port is free");
            exit::IO_ERROR
        }
    }
}

fn load_grants(path: &Path) -> Result<Grants, u8> {
    let text = fs::read_to_string(path).map_err(|e| {
        eprintln!(
            "error: cannot read grants '{}': {e}\n  fix: check the path to the grants file",
            path.display()
        );
        exit::IO_ERROR
    })?;
    parse_grants(&text).map_err(|why| {
        eprintln!(
            "error: invalid grants '{}'\n  why: {why}\n  fix: use the keys fs_read, net_connect and process_spawn, each a list",
            path.display()
        );
        exit::INVALID_POLICY
    })
}

fn check(
    request_path: &Path,
    policy_path: &Path,
    grants_path: Option<&Path>,
    format: Format,
) -> u8 {
    let policy = match load_policy(policy_path) {
        Ok(policy) => policy,
        Err(code) => return code,
    };
    let grants = match grants_path.map(load_grants).transpose() {
        Ok(grants) => grants,
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
                "error: the request '{}' is not a JSON object\n  fix: wrap the fields in {{ }}",
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
    let verdict = match decide_with_grants(&policy, grants.as_ref(), &request) {
        Ok(verdict) => verdict,
        Err(why) => {
            eprintln!("error: {why}");
            return exit::INVALID_INPUT;
        }
    };
    match format {
        Format::Json => println!(
            "{}",
            serde_json::to_string_pretty(&verdict).expect("verdict serialises")
        ),
        Format::Text => {
            println!("decision: {}", verdict.decision);
            println!("reason: {}", verdict.reason);
            println!("action: {}", verdict.action);
            println!("rules: {}", verdict.matched_rules.join(", "));
        }
    }
    match verdict.decision {
        "ALLOW" => exit::APPROVED,
        "REQUIRE_APPROVAL" => exit::APPROVAL_REQUIRED,
        _ => exit::REJECTED,
    }
}

fn validate(policy_path: &Path) -> u8 {
    let policy = match load_policy(policy_path) {
        Ok(policy) => policy,
        Err(code) => return code,
    };
    if let Err(why) = compile(&policy) {
        eprintln!("error: the policy does not compile\n  why: {why}");
        return exit::INVALID_POLICY;
    }
    println!(
        "policy ok: {} {} ({} rules)",
        policy.name,
        policy.version,
        policy.rules.len()
    );
    exit::SUCCESS
}

/// Built-in policy for the self-test: one allow, one approval, one deny.
const SELF_TEST_POLICY: &str = "rules:\n  - action: lookup\n    decision: allow\n  - action: refund\n    when:\n      amount: {gt: 100}\n    decision: require_approval\n  - action: delete\n    decision: deny\n";

/// `tpt-ai-guard doctor`: version, platform, a writable folder, and a self-test
/// that checks one request for each decision.
fn doctor() -> u8 {
    let mut failed = false;
    println!("[ok] version: tpt-ai-guard {GUARD_VERSION}");
    println!(
        "[ok] platform: {} {}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    let probe = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".tpt-ai-guard-doctor-probe");
    match fs::write(&probe, b"ok").and_then(|_| fs::remove_file(&probe)) {
        Ok(()) => println!("[ok] working directory is writable"),
        Err(e) => {
            println!("[fail] working directory is not writable: {e}. Fix: run from a folder you can write to");
            failed = true;
        }
    }
    let self_test = parse_action_policy(SELF_TEST_POLICY).map_err(|e| e.to_string()).and_then(|policy| {
        let allow = decide(&policy, &serde_json::json!({"action": "lookup"}))?.decision == "ALLOW";
        let approve = decide(&policy, &serde_json::json!({"action": "refund", "amount": 500}))?.decision == "REQUIRE_APPROVAL";
        let deny = decide(&policy, &serde_json::json!({"action": "delete"}))?.decision == "DENY";
        let default = decide(&policy, &serde_json::json!({"action": "unknown"}))?.decision == "DENY";
        if allow && approve && deny && default {
            Ok(())
        } else {
            Err("the built-in policy did not give the expected decisions; reinstall the release bundle".into())
        }
    });
    match self_test {
        Ok(()) => println!("[ok] self-test: built-in policy gives the expected decisions"),
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
