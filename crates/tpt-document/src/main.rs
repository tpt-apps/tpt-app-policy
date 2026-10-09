//! `tpt-document`: command-line front end for TPT Document Validator.
//!
//! Exit codes for this product, when every document was read:
//!
//! | Code | Meaning |
//! |---|---|
//! | 0 | Every document passed |
//! | 20 | At least one needs review, and none failed |
//! | 30 | At least one failed |
//! | 1 | A file could not be read or written. The run stops. |
//! | 2 | Invalid schema or policy, unknown document type, or a bad command line |
//!
//! The codes 20 and 30 match `tpt-policy`'s codes for review and rejected.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use serde_json::json;
use tpt_commercial_cli::log::{LogOptions, Logger};
use tpt_document::{
    check_document, file_sha256, read_document, DocError, DocFormat, DocResult, Parsed, Verdict,
};
use tpt_policy_core::{parse_policy, Policy};
use tpt_report::{document_report_html, DocumentReport, DocumentRow};
use tpt_schema::{parse_schema, Schema, SCHEMA_ENGINE_VERSION};

const EXIT_IO: u8 = 1;
const EXIT_USAGE: u8 = 2;

const AFTER_HELP: &str = "\
Quick start:
  tpt-document validate examples/documents/purchase-order.json --schema examples/documents/templates/purchase-order.schema.yaml
  tpt-document validate examples/documents/invoice.xml --schema examples/documents/templates/invoice.schema.yaml --out results --html

Each document gets PASS, REVIEW or FAIL. Several documents can be checked in one command.

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
    name = "tpt-document",
    version,
    about = "Check JSON and XML documents against a schema and a policy",
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
    /// Check one or more documents, and print a PASS, REVIEW or FAIL for each
    #[command(
        after_help = "Examples:\n  tpt-document validate examples/documents/purchase-order.json --schema examples/documents/templates/purchase-order.schema.yaml\n  tpt-document validate *.xml --schema shipping.schema.yaml --policy rules.yaml --out results\n\nPASS: passes the schema and the policy. REVIEW: a person should look. FAIL: fails the schema."
    )]
    Validate {
        /// The documents to check: .json or .xml. Give several to check them all at once
        #[arg(required = true, value_name = "DOCUMENT")]
        documents: Vec<PathBuf>,
        /// The schema (YAML) the document must match. Templates are in examples/documents/templates/
        #[arg(long, value_name = "FILE")]
        schema: PathBuf,
        /// Optional policy (YAML). Documents that pass the schema are also evaluated against it
        #[arg(long, value_name = "FILE")]
        policy: Option<PathBuf>,
        /// Folder for one <name>.result.json per document. Created if missing
        #[arg(long, value_name = "DIR")]
        out: Option<PathBuf>,
        /// Also write report.html into the --out folder: one page for people to read and file. Needs --out
        #[arg(long, requires = "out")]
        html: bool,
    },
    /// Check that this install works: version, platform, folders and a self-test
    #[command(
        after_help = "Example:\n  tpt-document doctor\n\nExits 5 if a required check fails."
    )]
    Doctor,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let log = Logger::new(cli.log);
    let started = log.started(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
    let code = match cli.command {
        Command::Validate {
            documents,
            schema,
            policy,
            out,
            html,
        } => validate(&documents, &schema, policy.as_deref(), out.as_deref(), html),
        Command::Doctor => doctor(),
    };
    log.finished(started, code);
    ExitCode::from(code)
}

fn validate(
    documents: &[PathBuf],
    schema_path: &Path,
    policy_path: Option<&Path>,
    out: Option<&Path>,
    html: bool,
) -> u8 {
    let schema = match load_schema(schema_path) {
        Ok(s) => s,
        Err(code) => return code,
    };
    let policy = match policy_path.map(load_policy).transpose() {
        Ok(p) => p,
        Err(code) => return code,
    };
    if let Some(dir) = out {
        if let Err(e) = fs::create_dir_all(dir) {
            eprintln!(
                "error: cannot create output folder '{}': {e}\n  fix: check the path is writable",
                dir.display()
            );
            return EXIT_IO;
        }
    }

    let mut worst = Verdict::Pass;
    let mut rows = Vec::new();
    for document in documents {
        let parsed = match read_document(document) {
            Ok(p) => p,
            Err(e) => return report_read_error(&e),
        };
        let sha = match file_sha256(document) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("error: cannot read '{}': {e}", document.display());
                return EXIT_IO;
            }
        };
        let result = check_document(&schema, policy.as_ref(), &parsed);
        worst = worst.worst(result.verdict);
        print_result(document, &result);
        if let Some(dir) = out {
            if let Err(code) = write_result(
                dir,
                document,
                &sha,
                &schema,
                policy.as_ref(),
                &parsed,
                &result,
            ) {
                return code;
            }
        }
        if html {
            rows.push(document_row(document, &parsed, &result));
        }
    }
    if let (true, Some(dir)) = (html, out) {
        let report = DocumentReport {
            schema_name: schema.name.clone(),
            schema_version: schema.version.clone(),
            policy: policy.as_ref().map(|p| (p.name.clone(), p.version.clone())),
            tool_version: env!("CARGO_PKG_VERSION").to_string(),
            documents: rows,
        };
        let path = dir.join("report.html");
        if let Err(e) = fs::write(&path, document_report_html(&report)) {
            eprintln!(
                "error: cannot write '{}': {e}\n  fix: check the folder is writable",
                path.display()
            );
            return EXIT_IO;
        }
    }
    worst.exit_code()
}

fn document_row(document: &Path, parsed: &Parsed, result: &DocResult) -> DocumentRow {
    let format = match parsed {
        Parsed::Ok { format, .. } | Parsed::Malformed { format, .. } => *format,
    };
    DocumentRow {
        file: document
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        format: format.name().to_string(),
        verdict: result.verdict.label().to_string(),
        decision: result.evaluation.as_ref().map(|ev| {
            if ev.approvers.is_empty() {
                ev.decision.to_string()
            } else {
                format!("{}, approvers: {}", ev.decision, ev.approvers.join(", "))
            }
        }),
        schema_errors: result
            .schema_errors
            .iter()
            .map(|e| (e.field.clone(), e.reason.clone()))
            .collect(),
        explanations: result
            .evaluation
            .iter()
            .flat_map(|ev| &ev.explanations)
            .map(|x| (x.rule.clone(), x.decision.to_string(), x.message.clone()))
            .collect(),
        warnings: result
            .evaluation
            .iter()
            .flat_map(|ev| ev.warnings.clone())
            .collect(),
    }
}

fn load_schema(path: &Path) -> Result<Schema, u8> {
    let text = fs::read_to_string(path).map_err(|e| {
        eprintln!("error: cannot read schema '{}': {e}", path.display());
        EXIT_IO
    })?;
    parse_schema(&text).map_err(|e| {
        eprintln!("{e}\n  file: {}", path.display());
        EXIT_USAGE
    })
}

fn load_policy(path: &Path) -> Result<Policy, u8> {
    let text = fs::read_to_string(path).map_err(|e| {
        eprintln!("error: cannot read policy '{}': {e}", path.display());
        EXIT_IO
    })?;
    parse_policy(&text).map_err(|e| {
        eprintln!("{e}\n  file: {}", path.display());
        EXIT_USAGE
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

fn print_result(document: &Path, result: &DocResult) {
    let name = document.display();
    let detail = match &result.evaluation {
        Some(ev) => {
            let approvers = if ev.approvers.is_empty() {
                String::new()
            } else {
                format!(", approvers: {}", ev.approvers.join(", "))
            };
            format!(" ({}{approvers})", ev.decision)
        }
        None => String::new(),
    };
    println!("{:<6} {name}{detail}", result.verdict.label());
    for error in &result.schema_errors {
        println!("       {}: {}", error.field, error.reason);
    }
    if let Some(ev) = &result.evaluation {
        for warning in &ev.warnings {
            println!("       warning: {warning}");
        }
    }
}

fn write_result(
    dir: &Path,
    document: &Path,
    sha: &str,
    schema: &Schema,
    policy: Option<&Policy>,
    parsed: &Parsed,
    result: &DocResult,
) -> Result<(), u8> {
    let format = match parsed {
        Parsed::Ok { format, .. } | Parsed::Malformed { format, .. } => *format,
    };
    let stem = document
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "document".to_string());
    let file = json!({
        "tool": "tpt-document",
        "tool_version": env!("CARGO_PKG_VERSION"),
        "document": {
            "file": document.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            "format": format.name(),
            "sha256": sha,
        },
        "schema": {
            "name": schema.name,
            "version": schema.version,
            "engine_version": SCHEMA_ENGINE_VERSION,
        },
        "policy": policy.map(|p| json!({ "name": p.name, "version": p.version })),
        "verdict": result.verdict.label(),
        "schema_errors": result.schema_errors,
        "evaluation": result.evaluation,
    });
    let path = dir.join(format!("{stem}.result.json"));
    let text = serde_json::to_string_pretty(&file).expect("result is serialisable");
    fs::write(&path, format!("{text}\n")).map_err(|e| {
        eprintln!(
            "error: cannot write '{}': {e}\n  fix: check the folder is writable",
            path.display()
        );
        EXIT_IO
    })
}

/// `tpt-document doctor`: version, platform, a writable folder, and a self-test
/// that parses a small XML document and checks it against a built-in schema.
fn doctor() -> u8 {
    const SCHEMA: &str =
        "schema: doctor\nfields:\n  total:\n    type: number\n    required: true\n";
    const XML: &str = "<doc><total>12.5</total></doc>";
    let mut failed = false;
    println!("[ok] version: tpt-document {}", env!("CARGO_PKG_VERSION"));
    println!(
        "[ok] platform: {} {}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    let probe = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".tpt-document-doctor-probe");
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
            let parsed = Parsed::Ok {
                format: DocFormat::Xml,
                value: tpt_document::xml::xml_to_value(XML).map_err(|e| e.message)?,
            };
            let result = check_document(&schema, None, &parsed);
            if result.verdict == Verdict::Pass {
                Ok(())
            } else {
                Err("the built-in document did not pass; reinstall the release bundle".to_string())
            }
        });
    match self_test {
        Ok(()) => println!("[ok] self-test: built-in XML document is checked correctly"),
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
