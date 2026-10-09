//! `tpt-transform`: command-line front end for TPT Data Transformer.
//!
//! Exit codes for this product:
//!
//! | Code | Meaning |
//! |---|---|
//! | 0 | Every record was transformed (filtered records are not errors) |
//! | 1 | A file could not be read or written |
//! | 2 | Invalid pipeline, unknown file type, or a bad command line |
//! | 3 | The input could not be read as a whole (for example a bad JSON array) |
//! | 10 | One or more records were rejected. Outputs were still written. |

use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use serde_json::{json, Value};
use tpt_commercial_cli::log::{LogOptions, Logger};
use tpt_data_core::{open, sha256_file, Format, ValidWriter};
use tpt_transform::{output_headers, parse_pipeline, run, Outcome, PipelineFile, Rejection};

const EXIT_OK: u8 = 0;
const EXIT_IO: u8 = 1;
const EXIT_USAGE: u8 = 2;
const EXIT_INPUT: u8 = 3;
const EXIT_REJECTED: u8 = 10;

/// How many rejected records to show on the console. The rest are in rejected.jsonl.
const CONSOLE_REJECTIONS: usize = 20;

const AFTER_HELP: &str = "\
Quick start:
  tpt-transform check --pipeline examples/transform/customers.pipeline.yaml
  tpt-transform run examples/transform/customers.csv --pipeline examples/transform/customers.pipeline.yaml --out results

A pipeline is a YAML list of steps. Records a step cannot handle go to the rejected file, with the reason.

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
    name = "tpt-transform",
    version,
    about = "Transform CSV, JSON and JSON Lines records with a repeatable pipeline",
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
    /// Run a pipeline over a file, and write the transformed and rejected records
    #[command(
        after_help = "Examples:\n  tpt-transform run examples/transform/customers.csv --pipeline examples/transform/customers.pipeline.yaml\n  tpt-transform run data.csv --pipeline steps.yaml --to jsonl --out results\n\nThe same pipeline and input always give the same output bytes."
    )]
    Run {
        /// The records to transform: .csv (with a header row), .json (an array) or .jsonl
        input: PathBuf,
        /// The pipeline (YAML): the steps to apply, in order
        #[arg(long, value_name = "FILE")]
        pipeline: PathBuf,
        /// Input format. Defaults to the file extension
        #[arg(long, value_name = "FORMAT")]
        format: Option<FormatArg>,
        /// Output format. Defaults to the input format
        #[arg(long, value_name = "FORMAT")]
        to: Option<FormatArg>,
        /// Folder for transformed and rejected records. Created if missing
        #[arg(long, value_name = "DIR", default_value = "tpt-transform-out")]
        out: PathBuf,
    },
    /// Check a pipeline file without running it
    #[command(
        after_help = "Example:\n  tpt-transform check --pipeline examples/transform/customers.pipeline.yaml\n\nReports the step and line of any error. Nothing is read from the data or written."
    )]
    Check {
        /// The pipeline (YAML) to check
        #[arg(long, value_name = "FILE")]
        pipeline: PathBuf,
    },
    /// Check that this install works: version, platform, folders and a self-test
    #[command(
        after_help = "Example:\n  tpt-transform doctor\n\nExits 5 if a required check fails."
    )]
    Doctor,
}

#[derive(Clone, Copy, ValueEnum)]
enum FormatArg {
    Csv,
    Json,
    Jsonl,
}

impl FormatArg {
    fn format(self) -> Format {
        match self {
            Self::Csv => Format::Csv,
            Self::Json => Format::Json,
            Self::Jsonl => Format::Jsonl,
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let log = Logger::new(cli.log);
    let started = log.started(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
    let code = match cli.command {
        Command::Run {
            input,
            pipeline,
            format,
            to,
            out,
        } => run_command(
            &input,
            &pipeline,
            format.map(FormatArg::format),
            to.map(FormatArg::format),
            &out,
        ),
        Command::Check { pipeline } => check(&pipeline),
        Command::Doctor => doctor(),
    };
    log.finished(started, code);
    ExitCode::from(code)
}

fn load_pipeline(path: &Path) -> Result<PipelineFile, u8> {
    let text = fs::read_to_string(path).map_err(|e| {
        eprintln!(
            "error: cannot read pipeline '{}': {e}\n  fix: check the path to the pipeline file",
            path.display()
        );
        EXIT_IO
    })?;
    let mut pipeline = parse_pipeline(&text).map_err(|why| {
        eprintln!(
            "error: invalid pipeline '{}'\n  why: {why}\n  fix: check the step name and its indentation",
            path.display()
        );
        EXIT_USAGE
    })?;
    // Lookup tables are read now, so a missing or bad table is reported before any record is read.
    let base = path.parent().unwrap_or_else(|| Path::new(""));
    pipeline.load_tables(base).map_err(|why| {
        eprintln!(
            "error: invalid pipeline '{}'\n  why: {why}\n  fix: check the table path is relative to the pipeline file, and the table has a unique key",
            path.display()
        );
        EXIT_USAGE
    })?;
    Ok(pipeline)
}

fn run_command(
    input: &Path,
    pipeline_path: &Path,
    format: Option<Format>,
    to: Option<Format>,
    out: &Path,
) -> u8 {
    let pipeline = match load_pipeline(pipeline_path) {
        Ok(p) => p,
        Err(code) => return code,
    };
    let Some(format) = format.or_else(|| Format::from_path(input)) else {
        eprintln!(
            "error: cannot tell the format of '{}'\n  fix: rename the file to .csv, .json or .jsonl, or pass --format",
            input.display()
        );
        return EXIT_USAGE;
    };
    let out_format = to.unwrap_or(format);

    let mut reader = match open(input, format) {
        Ok(reader) => reader,
        Err(e) => {
            eprintln!("{e}");
            return EXIT_INPUT;
        }
    };
    let input_headers = reader.headers.clone();
    let sha = match sha256_file(input) {
        Ok(sha) => sha,
        Err(e) => {
            eprintln!("error: cannot read '{}': {e}", input.display());
            return EXIT_IO;
        }
    };
    if let Err(e) = fs::create_dir_all(out) {
        eprintln!(
            "error: cannot create output folder '{}': {e}\n  fix: check the path is writable",
            out.display()
        );
        return EXIT_IO;
    }

    // Records are collected before writing, so the CSV columns are known in advance.
    let result = run(&pipeline, reader.by_ref());
    // A JSON array that breaks off part-way stops the run before anything is written.
    if let Some(e) = reader.failure() {
        eprintln!("{e}");
        return EXIT_INPUT;
    }
    let headers = output_headers(input_headers.as_deref(), &result.kept);

    let transformed_name = format!("transformed.{}", out_format.name());
    let transformed_path = out.join(&transformed_name);
    if let Err(code) = write_transformed(&transformed_path, out_format, &headers, &result.kept) {
        return code;
    }
    let rejected_path = out.join("rejected.jsonl");
    if let Err(code) = write_rejected(&rejected_path, &result.rejected) {
        return code;
    }

    let input_name = input
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let summary = json!({
        "tool": "tpt-transform",
        "tool_version": env!("CARGO_PKG_VERSION"),
        "pipeline": {
            "name": pipeline.name,
            "version": pipeline.version,
            "steps": pipeline.pipeline.len(),
        },
        "input": { "file": input_name, "format": format.name(), "sha256": sha },
        "output": { "file": transformed_name, "format": out_format.name() },
        "counts": {
            "read": result.read,
            "transformed": result.kept.len(),
            "filtered": result.filtered,
            "rejected": result.rejected.len(),
        },
    });
    let summary_path = out.join("summary.json");
    let text = serde_json::to_string_pretty(&summary).expect("summary is serialisable");
    if let Err(e) = fs::write(&summary_path, format!("{text}\n")) {
        eprintln!(
            "error: cannot write '{}': {e}\n  fix: check the folder is writable",
            summary_path.display()
        );
        return EXIT_IO;
    }

    println!(
        "transformed {} of {} records -> {}",
        result.kept.len(),
        result.read,
        transformed_path.display()
    );
    if result.filtered > 0 {
        println!("filtered {} (did not match a filter step)", result.filtered);
    }
    if result.rejected.is_empty() {
        return EXIT_OK;
    }
    println!(
        "rejected {} -> {}",
        result.rejected.len(),
        rejected_path.display()
    );
    for rejection in result.rejected.iter().take(CONSOLE_REJECTIONS) {
        println!("  row {}: {}", rejection.number, rejection.reason);
    }
    if result.rejected.len() > CONSOLE_REJECTIONS {
        println!(
            "  ... {} more in rejected.jsonl",
            result.rejected.len() - CONSOLE_REJECTIONS
        );
    }
    EXIT_REJECTED
}

fn write_transformed(
    path: &Path,
    format: Format,
    headers: &[String],
    kept: &[(u64, Value)],
) -> Result<(), u8> {
    let io_error = |e: std::io::Error| {
        eprintln!(
            "error: cannot write '{}': {e}\n  fix: check the folder is writable",
            path.display()
        );
        EXIT_IO
    };
    let mut writer = ValidWriter::create(path, format, headers).map_err(io_error)?;
    for (_, record) in kept {
        writer.write(record).map_err(io_error)?;
    }
    writer.finish().map_err(io_error)
}

fn write_rejected(path: &Path, rejected: &[Rejection]) -> Result<(), u8> {
    let io_error = |e: std::io::Error| {
        eprintln!(
            "error: cannot write '{}': {e}\n  fix: check the folder is writable",
            path.display()
        );
        EXIT_IO
    };
    let file = File::create(path).map_err(io_error)?;
    let mut file = BufWriter::new(file);
    for rejection in rejected {
        let line = json!({
            "row": rejection.number,
            "reason": rejection.reason,
            "input": rejection.record,
        });
        writeln!(file, "{line}").map_err(io_error)?;
    }
    file.flush().map_err(io_error)
}

fn check(pipeline_path: &Path) -> u8 {
    match load_pipeline(pipeline_path) {
        Ok(pipeline) => {
            println!(
                "pipeline ok: {} ({} steps)",
                pipeline.name.as_deref().unwrap_or("unnamed"),
                pipeline.pipeline.len()
            );
            EXIT_OK
        }
        Err(code) => code,
    }
}

/// `tpt-transform doctor`: version, platform, a writable folder, and a self-test
/// that runs a built-in pipeline over one record.
fn doctor() -> u8 {
    const PIPELINE: &str = "pipeline:\n  - trim:\n      fields: [\"*\"]\n  - to_number:\n      fields: [amount, fee]\n  - calculate:\n      into: total\n      operation: add\n      fields: [amount, fee]\n      decimals: 2\n";
    let mut failed = false;
    println!("[ok] version: tpt-transform {}", env!("CARGO_PKG_VERSION"));
    println!(
        "[ok] platform: {} {}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    let probe = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".tpt-transform-doctor-probe");
    match fs::write(&probe, b"ok").and_then(|_| fs::remove_file(&probe)) {
        Ok(()) => println!("[ok] working directory is writable"),
        Err(e) => {
            println!("[fail] working directory is not writable: {e}. Fix: run from a folder you can write to");
            failed = true;
        }
    }
    let self_test = parse_pipeline(PIPELINE)
        .map_err(|e| e.to_string())
        .and_then(|pipeline| {
            let record = json!({"amount": " 1.5 ", "fee": "2"});
            match pipeline.apply(&record)? {
                Outcome::Keep(out) if out["total"] == json!(3.5) => Ok(()),
                other => Err(format!("unexpected result: {other:?}")),
            }
        });
    match self_test {
        Ok(()) => println!("[ok] self-test: built-in pipeline transforms a record correctly"),
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
        EXIT_OK
    }
}
