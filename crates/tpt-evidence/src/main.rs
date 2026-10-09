//! `tpt-evidence`: command-line front end for TPT Compliance Evidence Processor.
//!
//! This tool organises and checks evidence. It does not certify compliance.
//!
//! Exit codes:
//!
//! | Code | Meaning |
//! |---|---|
//! | 0 | Complete: every control is covered and no evidence has an error |
//! | 1 | A file could not be read or written |
//! | 2 | Invalid evidence index, controls file, manifest, or a bad command line |
//! | 5 | `doctor` found a problem |
//! | 10 | `verify`: a file is missing or changed, or the manifest was edited |
//! | 20 | Incomplete: a control has no usable evidence, or an item has an error |

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use tpt_commercial_cli::exit;
use tpt_commercial_cli::log::{LogOptions, Logger};
use tpt_evidence::html::evidence_report_html;
use tpt_evidence::{
    parse_controls, parse_index, process, report_json, summary_lines, verify, Manifest,
    EVIDENCE_VERSION,
};

const EXIT_VERIFY_FAILED: u8 = 10;
const EXIT_INCOMPLETE: u8 = 20;

#[derive(Parser)]
#[command(
    name = "tpt-evidence",
    version,
    about = "Inventory, check, hash and report on compliance evidence. Does not certify compliance"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
    #[command(flatten)]
    log: LogOptions,
}

#[derive(Subcommand)]
enum Command {
    /// Check evidence against controls, and write the inventory, manifest and reports
    Process {
        /// Evidence index (YAML). File paths in it are relative to this file.
        #[arg(long, value_name = "FILE")]
        evidence: PathBuf,
        /// Controls file (YAML)
        #[arg(long, value_name = "FILE")]
        controls: PathBuf,
        /// The date to check ages against, as YYYY-MM-DD. Required, so results are repeatable.
        #[arg(long, value_name = "DATE")]
        as_of: String,
        /// Folder for report.json, report.html and manifest.json. Created if missing.
        #[arg(long, value_name = "DIR", default_value = "evidence-out")]
        out: PathBuf,
    },
    /// Check that the files in a manifest still match their hashes
    Verify {
        /// The manifest.json written by `process`
        #[arg(long, value_name = "FILE")]
        manifest: PathBuf,
        /// Folder the manifest paths are relative to (the evidence index folder)
        #[arg(long, value_name = "DIR")]
        root: PathBuf,
    },
    /// Check that this install works
    Doctor,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let log = Logger::new(cli.log);
    let started = log.started(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
    let code = match cli.command {
        Command::Process {
            evidence,
            controls,
            as_of,
            out,
        } => process_command(&evidence, &controls, &as_of, &out),
        Command::Verify { manifest, root } => verify_command(&manifest, &root),
        Command::Doctor => doctor(),
    };
    log.finished(started, code);
    ExitCode::from(code)
}

fn read(path: &Path, what: &str) -> Result<String, u8> {
    fs::read_to_string(path).map_err(|e| {
        eprintln!(
            "error: cannot read {what} '{}': {e}\n  fix: check the path",
            path.display()
        );
        exit::IO_ERROR
    })
}

fn process_command(evidence: &Path, controls_path: &Path, as_of: &str, out: &Path) -> u8 {
    let index = match read(evidence, "evidence index").and_then(|text| {
        parse_index(&text).map_err(|why| {
            eprintln!("error: invalid evidence index '{}'\n  why: {why}\n  fix: check the item and its date", evidence.display());
            exit::INVALID_POLICY
        })
    }) {
        Ok(index) => index,
        Err(code) => return code,
    };
    let controls = match read(controls_path, "controls file").and_then(|text| {
        parse_controls(&text).map_err(|why| {
            eprintln!(
                "error: invalid controls file '{}'\n  why: {why}",
                controls_path.display()
            );
            exit::INVALID_POLICY
        })
    }) {
        Ok(controls) => controls,
        Err(code) => return code,
    };
    let base = evidence
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let report = match process(&index, &controls, base, as_of) {
        Ok(report) => report,
        Err(why) => {
            eprintln!("error: {why}\n  fix: use a date in YYYY-MM-DD form, such as 2026-10-09");
            return exit::INVALID_POLICY;
        }
    };

    if let Err(e) = fs::create_dir_all(out) {
        eprintln!(
            "error: cannot create output folder '{}': {e}\n  fix: check the path is writable",
            out.display()
        );
        return exit::IO_ERROR;
    }
    let manifest_text = format!(
        "{}\n",
        serde_json::to_string_pretty(&report.manifest).expect("manifest serialises")
    );
    let files = [
        ("report.json", report_json(&report)),
        ("report.html", evidence_report_html(&report)),
        ("manifest.json", manifest_text),
    ];
    for (name, text) in files {
        let path = out.join(name);
        if let Err(e) = fs::write(&path, text) {
            eprintln!(
                "error: cannot write '{}': {e}\n  fix: check the folder is writable",
                path.display()
            );
            return exit::IO_ERROR;
        }
    }

    for line in summary_lines(&report) {
        println!("{line}");
    }
    println!(
        "wrote {}/report.json, report.html and manifest.json",
        out.display()
    );
    if report.status == "complete" {
        exit::SUCCESS
    } else {
        EXIT_INCOMPLETE
    }
}

fn verify_command(manifest_path: &Path, root: &Path) -> u8 {
    let text = match read(manifest_path, "manifest") {
        Ok(text) => text,
        Err(code) => return code,
    };
    let manifest: Manifest = match serde_json::from_str(&text) {
        Ok(manifest) => manifest,
        Err(e) => {
            eprintln!(
                "error: '{}' is not a manifest: {e}\n  fix: pass the manifest.json written by `tpt-evidence process`",
                manifest_path.display()
            );
            return exit::INVALID_POLICY;
        }
    };
    let (ok, lines) = verify(&manifest, root);
    for (path, status) in &lines {
        println!("{status:<8} {path}");
    }
    if ok {
        println!("all {} files match the manifest", manifest.entries.len());
        exit::SUCCESS
    } else {
        println!("verification failed");
        EXIT_VERIFY_FAILED
    }
}

/// `tpt-evidence doctor`: version, platform, a writable folder, and a self-test
/// that processes a small in-memory evidence set.
fn doctor() -> u8 {
    let mut failed = false;
    println!("[ok] version: tpt-evidence {EVIDENCE_VERSION}");
    println!(
        "[ok] platform: {} {}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    let probe = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join(".tpt-evidence-doctor-probe");
    match fs::write(&probe, b"ok").and_then(|_| fs::remove_file(&probe)) {
        Ok(()) => println!("[ok] working directory is writable"),
        Err(e) => {
            println!("[fail] working directory is not writable: {e}. Fix: run from a folder you can write to");
            failed = true;
        }
    }
    let self_test = (|| -> Result<(), String> {
        let index = parse_index("name: doctor\nitems:\n  - id: a\n    file: missing.txt\n    collected: 2026-09-01\n    controls: [C1]\n")?;
        let controls =
            parse_controls("framework: doctor\ncontrols:\n  - id: C1\n    title: Check\n")?;
        let report = process(&index, &controls, Path::new("."), "2026-10-09")?;
        if report.status == "incomplete"
            && report.items[0].status == "error"
            && report.controls[0].status == "gap"
        {
            Ok(())
        } else {
            Err("the built-in evidence set was not reported as expected; reinstall the release bundle".into())
        }
    })();
    match self_test {
        Ok(()) => println!("[ok] self-test: built-in evidence set is reported correctly"),
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
