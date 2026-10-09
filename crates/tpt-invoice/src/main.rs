//! `tpt-invoice`: command-line front end for TPT Invoice Validator.
//!
//! Exit codes for this product, when every invoice was read:
//!
//! | Code | Meaning |
//! |---|---|
//! | 0 | Every invoice passed |
//! | 20 | At least one needs review, and none were rejected |
//! | 30 | At least one was rejected |
//! | 1 | A file could not be read or written. The run stops. |
//! | 2 | Invalid schema, policy or supplier list, unknown document type, or a bad command line |

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use serde_json::json;
use tpt_commercial_cli::log::{LogOptions, Logger};
use tpt_document::{file_sha256, DocError, Parsed};
use tpt_invoice::{
    check_invoice, duplicate_key, read_invoices, InvoiceResult, Ledger, Suppliers, Verdict,
};
use tpt_policy_core::{parse_policy, Policy};
use tpt_schema::{parse_schema, Schema, SCHEMA_ENGINE_VERSION};

mod serve;

const EXIT_IO: u8 = 1;
const EXIT_USAGE: u8 = 2;

const AFTER_HELP: &str = "\
Quick start:
  tpt-invoice validate examples/invoices/invoice-ok.json --schema examples/invoices/invoice.schema.yaml --policy examples/invoices/invoice.policy.yaml --suppliers examples/invoices/suppliers.json
  tpt-invoice serve --schema examples/invoices/invoice.schema.yaml

Checks arithmetic (line totals, tax, grand total), approved suppliers and duplicate invoices.

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
    name = "tpt-invoice",
    version,
    about = "Check invoices for arithmetic, supplier and duplicate problems",
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
    /// Check one or more invoices (JSON, XML or CSV)
    #[command(
        after_help = "Examples:\n  tpt-invoice validate examples/invoices/invoice-ok.json --schema examples/invoices/invoice.schema.yaml\n  tpt-invoice validate examples/invoices/batch.csv --schema examples/invoices/invoice.schema.yaml --ledger processed.jsonl --out results\n\nThe ledger remembers accepted invoices. An invoice that is already in it is rejected as a duplicate, so run the same batch twice and the second run flags it."
    )]
    Validate {
        /// Invoices to check: .json or .xml (one invoice each), or .csv (one row per invoice line). Give several at once
        #[arg(required = true, value_name = "INVOICE")]
        invoices: Vec<PathBuf>,
        /// The schema (YAML) for the invoice header fields, such as number, date and supplier
        #[arg(long, value_name = "FILE")]
        schema: PathBuf,
        /// Optional business policy (YAML). Invoices that pass the checks are also evaluated against it
        #[arg(long, value_name = "FILE")]
        policy: Option<PathBuf>,
        /// Optional JSON array of approved supplier tax IDs. Invoices from other suppliers are flagged
        #[arg(long, value_name = "FILE")]
        suppliers: Option<PathBuf>,
        /// Optional JSON lines file of processed invoices. Duplicates are rejected, and each accepted invoice is added. Created if missing
        #[arg(long, value_name = "FILE")]
        ledger: Option<PathBuf>,
        /// Folder for one <name>.result.json per invoice. Created if missing
        #[arg(long, value_name = "DIR")]
        out: Option<PathBuf>,
    },
    /// Serve invoice checks over HTTP (POST /v1/validate)
    #[command(
        after_help = "Examples:\n  tpt-invoice serve --schema examples/invoices/invoice.schema.yaml\n  TPT_INVOICE_TOKEN=change-me tpt-invoice serve --schema invoice.schema.yaml --listen 0.0.0.0:8080 --ledger ledger.jsonl\n\nListens on 127.0.0.1:8080 by default. Requests are limited to 1 MB. The ledger is shared by all requests."
    )]
    Serve {
        /// The schema (YAML) for the invoice header fields
        #[arg(long, value_name = "FILE")]
        schema: PathBuf,
        /// Optional business policy (YAML)
        #[arg(long, value_name = "FILE")]
        policy: Option<PathBuf>,
        /// Optional JSON array of approved supplier tax IDs
        #[arg(long, value_name = "FILE")]
        suppliers: Option<PathBuf>,
        /// Optional JSON lines ledger of processed invoices, shared by all requests
        #[arg(long, value_name = "FILE")]
        ledger: Option<PathBuf>,
        /// Address and port to listen on. The default only accepts connections from this machine
        #[arg(long, default_value = "127.0.0.1:8080", value_name = "ADDR")]
        listen: String,
        /// Bearer token that callers must send. Set TPT_INVOICE_TOKEN instead of passing it here, so it stays out of shell history
        #[arg(long, env = "TPT_INVOICE_TOKEN", hide_env_values = true)]
        token: Option<String>,
    },
    /// Check that this install works: version, platform, folders and a self-test
    #[command(after_help = "Example:\n  tpt-invoice doctor\n\nExits 5 if a required check fails.")]
    Doctor,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let log = Logger::new(cli.log);
    let started = log.started(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
    let code = match cli.command {
        Command::Validate {
            invoices,
            schema,
            policy,
            suppliers,
            ledger,
            out,
        } => validate(
            &invoices,
            &Inputs {
                schema: &schema,
                policy: policy.as_deref(),
                suppliers: suppliers.as_deref(),
                ledger: ledger.as_deref(),
                out: out.as_deref(),
            },
        ),
        Command::Serve {
            schema,
            policy,
            suppliers,
            ledger,
            listen,
            token,
        } => serve_invoices(
            &Inputs {
                schema: &schema,
                policy: policy.as_deref(),
                suppliers: suppliers.as_deref(),
                ledger: ledger.as_deref(),
                out: None,
            },
            &listen,
            token,
        ),
        Command::Doctor => doctor(),
    };
    log.finished(started, code);
    ExitCode::from(code)
}

struct Inputs<'a> {
    schema: &'a Path,
    policy: Option<&'a Path>,
    suppliers: Option<&'a Path>,
    ledger: Option<&'a Path>,
    out: Option<&'a Path>,
}

fn validate(invoices: &[PathBuf], inputs: &Inputs) -> u8 {
    let Ok(schema) = load_schema(inputs.schema) else {
        return EXIT_USAGE;
    };
    let Ok(policy) = inputs.policy.map(load_policy).transpose() else {
        return EXIT_USAGE;
    };
    let Ok(suppliers) = inputs.suppliers.map(load_suppliers).transpose() else {
        return EXIT_USAGE;
    };
    let mut ledger = match inputs.ledger.map(Ledger::load).transpose() {
        Ok(l) => l,
        Err(e) => {
            eprintln!("error: cannot read ledger: {e}\n  fix: check the file is JSON lines, or remove it to start a new ledger");
            return EXIT_IO;
        }
    };
    if let Some(dir) = inputs.out {
        if let Err(e) = fs::create_dir_all(dir) {
            eprintln!(
                "error: cannot create output folder '{}': {e}",
                dir.display()
            );
            return EXIT_IO;
        }
    }

    let mut worst = Verdict::Pass;
    for path in invoices {
        let found = match read_invoices(path) {
            Ok(found) => found,
            Err(e) => return report_read_error(&e),
        };
        let sha = match file_sha256(path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("error: cannot read '{}': {e}", path.display());
                return EXIT_IO;
            }
        };
        for invoice in found {
            let result = check_invoice(
                &schema,
                policy.as_ref(),
                suppliers.as_ref(),
                ledger.as_ref(),
                &invoice.parsed,
            );

            // Accepted invoices join the ledger at once, so a repeat later in this run is caught.
            if let (Some(ledger), Parsed::Ok { value, .. }) = (ledger.as_mut(), &invoice.parsed) {
                if result.verdict != Verdict::Reject {
                    if let Some(key) = duplicate_key(value) {
                        if let Err(e) = ledger.record(&key) {
                            eprintln!(
                                "error: cannot write ledger: {e}\n  fix: check the file is writable"
                            );
                            return EXIT_IO;
                        }
                    }
                }
            }

            worst = worst.worst(result.verdict);
            print_result(&invoice.label, &result);
            if let Some(dir) = inputs.out {
                let file_name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let source = Source {
                    file_name: &file_name,
                    stem: &invoice.out_stem,
                    sha: &sha,
                };
                if let Err(code) = write_result(dir, &source, &schema, policy.as_ref(), &result) {
                    return code;
                }
            }
        }
    }
    worst.exit_code()
}

/// `tpt-invoice serve`: load the inputs once, then check invoices over HTTP
/// until stopped.
fn serve_invoices(inputs: &Inputs, listen: &str, token: Option<String>) -> u8 {
    let Ok(schema) = load_schema(inputs.schema) else {
        return EXIT_USAGE;
    };
    let Ok(policy) = inputs.policy.map(load_policy).transpose() else {
        return EXIT_USAGE;
    };
    let Ok(suppliers) = inputs.suppliers.map(load_suppliers).transpose() else {
        return EXIT_USAGE;
    };
    let ledger = match inputs.ledger.map(Ledger::load).transpose() {
        Ok(l) => l,
        Err(e) => {
            eprintln!("error: cannot read ledger: {e}\n  fix: check the file is JSON lines, or remove it to start a new ledger");
            return EXIT_IO;
        }
    };
    let is_loopback = {
        let host = listen.rsplit_once(':').map_or(listen, |(host, _)| host);
        matches!(host, "127.0.0.1" | "localhost" | "[::1]" | "::1")
    };
    if token.is_none() && !is_loopback {
        eprintln!(
            "warning: listening on {listen} without a token; anyone who can reach this address can submit invoices. Set TPT_INVOICE_TOKEN or listen on 127.0.0.1."
        );
    }
    let service = serve::Service {
        schema,
        policy,
        suppliers,
        ledger,
        token,
    };
    match serve::serve(service, listen) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("error: cannot serve on {listen}: {e}\n  fix: check the address is valid and the port is free");
            EXIT_IO
        }
    }
}

/// Where an invoice came from, for its result file.
struct Source<'a> {
    /// The file the invoice was read from.
    file_name: &'a str,
    /// The name for the result file, without `.result.json`.
    stem: &'a str,
    /// SHA-256 of the file the invoice was read from.
    sha: &'a str,
}

fn load_schema(path: &Path) -> Result<Schema, ()> {
    let text = fs::read_to_string(path).map_err(|e| {
        eprintln!("error: cannot read schema '{}': {e}", path.display());
    })?;
    parse_schema(&text).map_err(|e| {
        eprintln!("{e}\n  file: {}", path.display());
    })
}

fn load_policy(path: &Path) -> Result<Policy, ()> {
    let text = fs::read_to_string(path).map_err(|e| {
        eprintln!("error: cannot read policy '{}': {e}", path.display());
    })?;
    parse_policy(&text).map_err(|e| {
        eprintln!("{e}\n  file: {}", path.display());
    })
}

fn load_suppliers(path: &Path) -> Result<Suppliers, ()> {
    let text = fs::read_to_string(path).map_err(|e| {
        eprintln!("error: cannot read supplier list '{}': {e}", path.display());
    })?;
    Suppliers::parse(&text).map_err(|e| {
        eprintln!("error: {e}\n  file: {}", path.display());
    })
}

fn report_read_error(e: &DocError) -> u8 {
    eprintln!("{e}");
    if e.what == "cannot read document" {
        EXIT_IO
    } else {
        EXIT_USAGE
    }
}

fn print_result(label: &str, result: &InvoiceResult) {
    let mut detail = String::new();
    if let Some(ev) = &result.evaluation {
        detail.push_str(&format!(" ({})", ev.decision));
    }
    println!("{:<6} {label}{detail}", result.verdict.label());
    for error in &result.schema_errors {
        println!("       schema {}: {}", error.field, error.reason);
    }
    for check in &result.checks {
        let mark = if check.passed { "ok  " } else { "FAIL" };
        println!("       {mark} {:<10} {}", check.name, check.message);
    }
    if let Some(ev) = &result.evaluation {
        if !ev.approvers.is_empty() {
            println!("       approvers: {}", ev.approvers.join(", "));
        }
        for warning in &ev.warnings {
            println!("       warning: {warning}");
        }
    }
}

fn write_result(
    dir: &Path,
    source: &Source,
    schema: &Schema,
    policy: Option<&Policy>,
    result: &InvoiceResult,
) -> Result<(), u8> {
    let file = json!({
        "tool": "tpt-invoice",
        "tool_version": env!("CARGO_PKG_VERSION"),
        "invoice": {
            "file": source.file_name,
            "sha256": source.sha,
        },
        "schema": {
            "name": schema.name,
            "version": schema.version,
            "engine_version": SCHEMA_ENGINE_VERSION,
        },
        "policy": policy.map(|p| json!({ "name": p.name, "version": p.version })),
        "result": result,
    });
    let out = dir.join(format!("{}.result.json", source.stem));
    let text = serde_json::to_string_pretty(&file).expect("result is serialisable");
    fs::write(&out, format!("{text}\n")).map_err(|e| {
        eprintln!(
            "error: cannot write '{}': {e}\n  fix: check the folder is writable",
            out.display()
        );
        EXIT_IO
    })
}

/// `tpt-invoice doctor`: version, platform, a writable folder, and a self-test
/// that checks a built-in invoice's arithmetic.
fn doctor() -> u8 {
    const SCHEMA: &str =
        "schema: doctor\nfields:\n  total:\n    type: number\n    required: true\n";
    const GOOD: &str = r#"{"total": 115, "subtotal": 100, "tax": 15, "tax_rate": 0.15, "lines": [{"quantity": 2, "unit_price": 50, "amount": 100}]}"#;
    const BAD: &str = r#"{"total": 120, "subtotal": 100, "tax": 15, "tax_rate": 0.15, "lines": [{"quantity": 2, "unit_price": 50, "amount": 100}]}"#;
    let mut failed = false;
    println!("[ok] version: tpt-invoice {}", env!("CARGO_PKG_VERSION"));
    println!(
        "[ok] platform: {} {}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    let probe = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".tpt-invoice-doctor-probe");
    match fs::write(&probe, b"ok").and_then(|_| fs::remove_file(&probe)) {
        Ok(()) => println!("[ok] working directory is writable"),
        Err(e) => {
            println!("[fail] working directory is not writable: {e}. Fix: run from a folder you can write to");
            failed = true;
        }
    }
    let self_test = parse_schema(SCHEMA)
        .map_err(|e| e.to_string())
        .and_then(|schema| {
            let check = |text: &str| -> Result<Verdict, String> {
                let value: serde_json::Value =
                    serde_json::from_str(text).map_err(|e| e.to_string())?;
                let parsed = Parsed::Ok {
                    format: tpt_document::DocFormat::Json,
                    value,
                };
                Ok(check_invoice(&schema, None, None, None, &parsed).verdict)
            };
            if check(GOOD)? == Verdict::Pass && check(BAD)? == Verdict::Reject {
                Ok(())
            } else {
                Err(
                    "the built-in invoices gave the wrong verdicts; reinstall the release bundle"
                        .to_string(),
                )
            }
        });
    match self_test {
        Ok(()) => println!("[ok] self-test: built-in arithmetic checks are correct"),
        Err(why) => {
            println!("[fail] self-test: {why}");
            failed = true;
        }
    }
    println!();
    if failed {
        println!("some checks failed");
        5
    } else {
        println!("all checks passed");
        0
    }
}
