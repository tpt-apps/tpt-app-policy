//! `tpt-data`: command-line front end for TPT Data Validator.
//!
//! Exit codes for this product:
//!
//! | Code | Meaning |
//! |---|---|
//! | 0 | Every record is valid |
//! | 1 | A file could not be read or written |
//! | 2 | Invalid schema or policy, or a bad command line |
//! | 3 | The input could not be read as a whole (for example a bad JSON array) |
//! | 10 | One or more records are invalid. Outputs were still written. |

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};
use serde::Serialize;
use tpt_data_core::report::row_block;
use tpt_data_core::validate::Summary;
use tpt_data_core::{open, sha256_file, DataError, Format, InvalidWriter, ValidWriter, Validator};
use tpt_policy_core::{parse_policy, Policy};
use tpt_report::{data_report_html, DataReport, ReportRow};
use tpt_schema::{parse_schema, Mode, Schema, SCHEMA_ENGINE_VERSION};

const EXIT_OK: u8 = 0;
const EXIT_IO: u8 = 1;
const EXIT_USAGE: u8 = 2;
const EXIT_INPUT: u8 = 3;
const EXIT_INVALID_RECORDS: u8 = 10;

/// How many invalid records to show on the console. The rest are in errors.txt.
const CONSOLE_ERRORS: usize = 20;

/// How many invalid records the HTML report lists. The rest are in invalid.jsonl.
const HTML_ROWS: usize = 500;

#[derive(Parser)]
#[command(
    name = "tpt-data",
    version,
    about = "Check CSV, JSON and JSON Lines records against a schema"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Validate a file against a schema, and write valid and invalid records
    Validate {
        /// Input file: .csv, .json (an array) or .jsonl
        input: PathBuf,
        /// Schema file (YAML)
        #[arg(long, value_name = "FILE")]
        schema: PathBuf,
        /// Optional policy file. Valid records are also evaluated against it.
        #[arg(long, value_name = "FILE")]
        policy: Option<PathBuf>,
        /// Input format. Defaults to the file extension.
        #[arg(long, value_enum)]
        format: Option<FormatArg>,
        /// Folder for the output files. Created if missing.
        #[arg(long, value_name = "DIR", default_value = "tpt-data-out")]
        out: PathBuf,
        /// Also write report.html, a self-contained page for people to read
        #[arg(long)]
        html: bool,
    },
    /// Check that this install works
    Doctor,
}

#[derive(Clone, Copy, ValueEnum)]
enum FormatArg {
    Csv,
    Json,
    Jsonl,
}

impl From<FormatArg> for Format {
    fn from(arg: FormatArg) -> Self {
        match arg {
            FormatArg::Csv => Format::Csv,
            FormatArg::Json => Format::Json,
            FormatArg::Jsonl => Format::Jsonl,
        }
    }
}

fn main() -> ExitCode {
    let code = match Cli::parse().command {
        Command::Validate {
            input,
            schema,
            policy,
            format,
            out,
            html,
        } => match validate(&input, &schema, policy.as_deref(), format, &out, html) {
            Ok(code) => code,
            Err(code) => code,
        },
        Command::Doctor => doctor(),
    };
    ExitCode::from(code)
}

fn validate(
    input: &Path,
    schema_path: &Path,
    policy_path: Option<&Path>,
    format: Option<FormatArg>,
    out: &Path,
    html: bool,
) -> Result<u8, u8> {
    let schema = load_schema(schema_path)?;
    let policy = policy_path.map(load_policy).transpose()?;

    let format = match format {
        Some(arg) => arg.into(),
        None => Format::from_path(input).ok_or_else(|| {
            eprintln!(
                "error: cannot tell the format of '{}'\n  why: the extension is not .csv, .json or .jsonl\n  fix: pass --format csv, json or jsonl",
                input.display()
            );
            EXIT_USAGE
        })?,
    };
    // CSV cells are text, so they are read leniently. JSON values must already have the right type.
    let mode = match format {
        Format::Csv => Mode::Text,
        Format::Json | Format::Jsonl => Mode::Strict,
    };

    fs::create_dir_all(out).map_err(|e| io_error(out, "cannot create output folder", &e))?;
    let input_sha = sha256_file(input).map_err(|e| io_error(input, "cannot read input", &e))?;
    let mut items = open(input, format).map_err(|e| data_error(&e))?;

    let headers = items.headers.clone().unwrap_or_default();
    let mut valid_out = ValidWriter::create(&out.join(valid_name(format)), format, &headers)
        .map_err(|e| io_error(out, "cannot write valid records", &e))?;
    let mut invalid_out = InvalidWriter::create(&out.join("invalid.jsonl"))
        .map_err(|e| io_error(out, "cannot write invalid records", &e))?;
    let mut errors_out = File::create(out.join("errors.txt"))
        .map_err(|e| io_error(out, "cannot write error report", &e))?;

    let mut validator = Validator::new(&schema, policy.as_ref(), mode);
    let mut summary = Summary::default();
    let mut shown = Vec::new();
    let mut html_rows = Vec::new();

    for item in items.by_ref() {
        let outcome = validator.check(item);
        summary.add(&outcome);
        if outcome.is_valid() {
            if let Some(record) = &outcome.record {
                valid_out
                    .write(record)
                    .map_err(|e| io_error(out, "cannot write valid records", &e))?;
            }
        } else {
            invalid_out
                .write(&outcome)
                .map_err(|e| io_error(out, "cannot write invalid records", &e))?;
            let block = row_block(&outcome);
            errors_out
                .write_all(block.as_bytes())
                .map_err(|e| io_error(out, "cannot write error report", &e))?;
            if shown.len() < CONSOLE_ERRORS {
                shown.push(block);
            }
            if html && html_rows.len() < HTML_ROWS {
                html_rows.push(ReportRow {
                    number: outcome.number,
                    errors: outcome
                        .errors
                        .iter()
                        .map(|e| (e.field.clone(), e.reason.clone()))
                        .collect(),
                });
            }
        }
    }

    // A JSON array that breaks off part-way is an input error. Records before
    // the break are already written, so the outputs are partial. The summary is not.
    if let Some(e) = items.failure() {
        return Err(data_error(&e));
    }

    valid_out
        .finish()
        .map_err(|e| io_error(out, "cannot finish valid records", &e))?;
    invalid_out
        .finish()
        .map_err(|e| io_error(out, "cannot finish invalid records", &e))?;

    write_summary(
        &out.join("summary.json"),
        &schema,
        policy.as_ref(),
        input,
        format,
        &input_sha,
        &summary,
    )?;

    if html {
        let report = DataReport {
            schema_name: schema.name.clone(),
            schema_version: schema.version.clone(),
            policy: policy.as_ref().map(|p| (p.name.clone(), p.version.clone())),
            input_file: input
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            input_format: format.name().to_string(),
            input_sha256: input_sha.clone(),
            processed: summary.processed,
            valid: summary.valid,
            invalid: summary.invalid,
            errors_by_field: sorted_by_count(&summary.errors_by_field),
            decisions: sorted_by_count(&summary.decisions),
            rows_omitted: summary.invalid - html_rows.len() as u64,
            rows: html_rows,
        };
        fs::write(out.join("report.html"), data_report_html(&report))
            .map_err(|e| io_error(out, "cannot write HTML report", &e))?;
    }

    print_console(&summary, &shown, out);
    Ok(if summary.invalid == 0 {
        EXIT_OK
    } else {
        EXIT_INVALID_RECORDS
    })
}

fn valid_name(format: Format) -> String {
    format!("valid.{}", format.name())
}

fn load_schema(path: &Path) -> Result<Schema, u8> {
    let text = fs::read_to_string(path).map_err(|e| io_error(path, "cannot read schema", &e))?;
    parse_schema(&text).map_err(|e| {
        eprintln!("{e}\n  file: {}", path.display());
        EXIT_USAGE
    })
}

fn load_policy(path: &Path) -> Result<Policy, u8> {
    let text = fs::read_to_string(path).map_err(|e| io_error(path, "cannot read policy", &e))?;
    parse_policy(&text).map_err(|e| {
        eprintln!("{e}\n  file: {}", path.display());
        EXIT_USAGE
    })
}

fn io_error(path: &Path, what: &str, e: &std::io::Error) -> u8 {
    eprintln!(
        "error: {what} '{}'\n  why: {e}\n  fix: check the path is right and the folder is writable",
        path.display()
    );
    EXIT_IO
}

fn data_error(e: &DataError) -> u8 {
    eprintln!("{e}");
    match e.what.as_str() {
        "cannot read input" => EXIT_IO,
        _ => EXIT_INPUT,
    }
}

#[derive(Serialize)]
struct SummaryFile<'a> {
    tool: &'static str,
    tool_version: &'static str,
    schema: SchemaRef<'a>,
    policy: Option<PolicyRef<'a>>,
    input: InputRef<'a>,
    result: &'a Summary,
}

#[derive(Serialize)]
struct SchemaRef<'a> {
    name: &'a str,
    version: &'a str,
    engine_version: &'static str,
}

#[derive(Serialize)]
struct PolicyRef<'a> {
    name: &'a str,
    version: &'a str,
}

#[derive(Serialize)]
struct InputRef<'a> {
    file: String,
    format: &'static str,
    sha256: &'a str,
}

fn write_summary(
    path: &Path,
    schema: &Schema,
    policy: Option<&Policy>,
    input: &Path,
    format: Format,
    input_sha: &str,
    summary: &Summary,
) -> Result<(), u8> {
    let file = SummaryFile {
        tool: "tpt-data",
        tool_version: env!("CARGO_PKG_VERSION"),
        schema: SchemaRef {
            name: &schema.name,
            version: &schema.version,
            engine_version: SCHEMA_ENGINE_VERSION,
        },
        policy: policy.map(|p| PolicyRef {
            name: &p.name,
            version: &p.version,
        }),
        input: InputRef {
            file: input
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            format: format.name(),
            sha256: input_sha,
        },
        result: summary,
    };
    let text = serde_json::to_string_pretty(&file).expect("summary is serialisable");
    fs::write(path, format!("{text}\n")).map_err(|e| io_error(path, "cannot write summary", &e))
}

fn print_console(summary: &Summary, shown: &[String], out: &Path) {
    println!("{} records processed", group(summary.processed));
    println!();
    println!("{} valid", group(summary.valid));
    println!("{} invalid", group(summary.invalid));
    if !summary.decisions.is_empty() {
        println!();
        println!("policy decisions for valid records:");
        for (decision, count) in &summary.decisions {
            println!("  {decision}: {}", group(*count));
        }
    }
    if summary.invalid > 0 {
        println!();
        for block in shown {
            print!("{block}");
        }
        let hidden = summary.invalid as usize - shown.len();
        if hidden > 0 {
            println!("... {hidden} more in errors.txt");
        }
    }
    println!();
    println!("written to {}", out.display());
}

/// Most frequent first, then by name, so the order is the same on every run.
fn sorted_by_count(map: &std::collections::BTreeMap<String, u64>) -> Vec<(String, u64)> {
    let mut items: Vec<(String, u64)> = map.iter().map(|(k, v)| (k.clone(), *v)).collect();
    items.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    items
}

/// Thousands separators, so 1247 reads as 1,247.
fn group(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// `tpt-data doctor`: version, platform, a writable folder, and a self-test
/// that checks one record against a built-in schema.
fn doctor() -> u8 {
    const SCHEMA: &str = "schema: doctor\nfields:\n  id:\n    type: integer\n    required: true\n";
    let mut failed = false;
    println!("[ok] version: tpt-data {}", env!("CARGO_PKG_VERSION"));
    println!(
        "[ok] platform: {} {}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    let probe = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".tpt-data-doctor-probe");
    match fs::write(&probe, b"ok").and_then(|_| fs::remove_file(&probe)) {
        Ok(()) => println!("[ok] working directory is writable"),
        Err(e) => {
            println!("[fail] working directory is not writable: {e}. Fix: run from a folder you can write to");
            failed = true;
        }
    }
    let self_test = match parse_schema(SCHEMA) {
        Err(e) => Err(e.to_string()),
        Ok(schema) => {
            let check = |record: serde_json::Value| {
                tpt_data_core::validate::Validator::new(&schema, None, Mode::Strict).check(
                    tpt_data_core::Item::Row(tpt_data_core::Row { number: 1, record }),
                )
            };
            if check(serde_json::json!({ "id": 7 })).is_valid()
                && !check(serde_json::json!({ "id": "seven" })).is_valid()
            {
                Ok(())
            } else {
                Err(
                    "the built-in check gave the wrong answer; reinstall the release bundle"
                        .to_string(),
                )
            }
        }
    };
    match self_test {
        Ok(()) => println!("[ok] self-test: built-in schema checks records correctly"),
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
