//! `tpt-secure-run`: command-line front end for TPT Secure Script Runner.
//!
//! Exit codes:
//!
//! | Code | Meaning |
//! |---|---|
//! | 0 | The script returned 0 (`validate`: the manifest is valid, and the module would not be denied) |
//! | 1 | A file could not be read or written |
//! | 2 | Invalid manifest, or a bad command line |
//! | 3 | The module is not valid WebAssembly, or has no `run` export |
//! | 4 | The script trapped, or used up its step, memory or call-depth limit |
//! | 5 | `doctor` found a problem |
//! | 6 | The script returned a non-zero status |
//! | 7 | The script imports something the manifest does not grant (it never ran) |

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use tpt_commercial_cli::exit;
use tpt_commercial_cli::log::{LogOptions, Logger};
use tpt_secure_run::codes;
use tpt_secure_run::manifest::{parse_manifest, Manifest};
use tpt_secure_run::{check_module, run_module, self_test, PRODUCT, RUNNER_VERSION};

const AFTER_HELP: &str = "\
Quick start:
  tpt-secure-run validate examples/secure-run/manifest.yaml --module examples/secure-run/count-lines.wasm
  tpt-secure-run run examples/secure-run/manifest.yaml examples/secure-run/count-lines.wasm

A script can read only the files its manifest grants, and has no network or process access unless the manifest grants it. Each run ends with a JSON report.

Exit codes (not every command uses every code):
  0   Success. The script ran, or the manifest is valid
  1   A file could not be read or written
  2   The manifest is invalid, or the command line is wrong
  3   The module is not valid WebAssembly, or has no 'run' export
  4   The script trapped, or used up a limit (steps, memory or call depth)
  5   doctor found a failed check
  6   The script returned a non-zero status
  7   The script imports something the manifest does not grant

Logs (stderr only, never input values): --verbose, --debug, --json-logs.
Every command has its own --help, with examples.";

#[derive(Parser)]
#[command(
    name = "tpt-secure-run",
    version,
    about = "Run a WebAssembly script with only the file access its manifest grants",
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
    /// Run a script under its manifest and print a JSON report
    #[command(
        after_help = "Example:\n  tpt-secure-run run examples/secure-run/manifest.yaml examples/secure-run/count-lines.wasm --output report.json\n\nThe JSON report records the script, its status, the limits it ran under, the imports granted, and the bytes it read and wrote."
    )]
    Run {
        /// The permission manifest (YAML): what the script may read, write and call. Relative paths are taken from the manifest's folder
        manifest: PathBuf,
        /// The script, as a WebAssembly binary (.wasm). The .wat files in examples/secure-run are the readable source
        module: PathBuf,
        /// Write the JSON report to this file instead of stdout
        #[arg(long, value_name = "FILE")]
        output: Option<PathBuf>,
    },
    /// Check a manifest, and optionally a module against it, without running anything
    #[command(
        after_help = "Examples:\n  tpt-secure-run validate examples/secure-run/manifest.yaml\n  tpt-secure-run validate examples/secure-run/manifest.yaml --module examples/secure-run/denied-network.wasm\n\nWith --module, the script's imports are checked against the manifest. A script that asks for a permission the manifest does not grant fails here, before it runs."
    )]
    Validate {
        /// The permission manifest (YAML) to check
        manifest: PathBuf,
        /// Also check that the script's imports are all granted by the manifest
        #[arg(long, value_name = "FILE")]
        module: Option<PathBuf>,
    },
    /// Check that this install works: version, platform, folders and a self-test
    #[command(
        after_help = "Example:\n  tpt-secure-run doctor\n\nExits 5 if a required check fails."
    )]
    Doctor,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let log = Logger::new(cli.log);
    let started = log.started(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
    let code = match cli.command {
        Command::Run {
            manifest,
            module,
            output,
        } => run(&manifest, &module, output.as_deref()),
        Command::Validate { manifest, module } => validate(&manifest, module.as_deref()),
        Command::Doctor => doctor(),
    };
    log.finished(started, code);
    ExitCode::from(code)
}

/// The folder that relative manifest paths are taken from.
fn base_dir(manifest: &Path) -> PathBuf {
    match manifest.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    }
}

fn load_manifest(path: &Path) -> Result<Manifest, u8> {
    let text = fs::read_to_string(path).map_err(|e| {
        eprintln!("error: cannot read {}: {e}", path.display());
        exit::IO_ERROR
    })?;
    parse_manifest(&text).map_err(|e| {
        eprintln!("error: {}: {e}", path.display());
        exit::INVALID_POLICY
    })
}

fn read_module(path: &Path) -> Result<Vec<u8>, u8> {
    fs::read(path).map_err(|e| {
        eprintln!("error: cannot read {}: {e}", path.display());
        exit::IO_ERROR
    })
}

fn run(manifest_path: &Path, module_path: &Path, output: Option<&Path>) -> u8 {
    let manifest = match load_manifest(manifest_path) {
        Ok(m) => m,
        Err(code) => return code,
    };
    let wasm = match read_module(module_path) {
        Ok(w) => w,
        Err(code) => return code,
    };
    let report = run_module(&manifest, &base_dir(manifest_path), &wasm);
    let json = match serde_json::to_string_pretty(&report) {
        Ok(json) => json,
        Err(e) => {
            eprintln!("error: cannot write report: {e}");
            return exit::IO_ERROR;
        }
    };
    match output {
        Some(path) => {
            if let Err(e) = fs::write(path, format!("{json}\n")) {
                eprintln!("error: cannot write {}: {e}", path.display());
                return exit::IO_ERROR;
            }
        }
        None => println!("{json}"),
    }
    if let Some(reason) = &report.reason {
        if report.exit_code() != exit::SUCCESS {
            eprintln!("{}: {reason}", report.script);
        }
    }
    report.exit_code()
}

fn validate(manifest_path: &Path, module_path: Option<&Path>) -> u8 {
    let manifest = match load_manifest(manifest_path) {
        Ok(m) => m,
        Err(code) => return code,
    };
    println!("manifest ok: {}", manifest.name);
    let Some(module_path) = module_path else {
        return exit::SUCCESS;
    };
    let wasm = match read_module(module_path) {
        Ok(w) => w,
        Err(code) => return code,
    };
    match check_module(&manifest, &wasm) {
        Err(e) => {
            eprintln!(
                "error: {} is not a valid module: {e}",
                module_path.display()
            );
            codes::INVALID_MODULE
        }
        Ok(denials) if denials.is_empty() => {
            println!("module ok: every import is granted");
            exit::SUCCESS
        }
        Ok(denials) => {
            for d in &denials {
                eprintln!("denied: {d}");
            }
            codes::DENIED
        }
    }
}

fn doctor() -> u8 {
    println!("{PRODUCT} {RUNNER_VERSION}");
    println!(
        "platform: {} {}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    let failures = self_test();
    if failures.is_empty() {
        println!("self-test: ok (pure script runs; ungranted import is refused)");
        exit::SUCCESS
    } else {
        for f in &failures {
            println!("FAIL: {f}");
        }
        exit::DOCTOR_FAILED
    }
}
