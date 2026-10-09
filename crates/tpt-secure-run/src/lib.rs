//! `tpt-secure-run`: runs a WebAssembly script with only the authority its manifest grants.
//!
//! The script runs in a deterministic `tpt-wasm` engine with the step, memory
//! and call-depth limits from its manifest. It can reach the outside world only
//! through host functions. Each import is checked against the grants before the
//! script runs, and a script that imports something ungranted is refused.
//!
//! The run's lifecycle follows the `tpt-runtime-core` workload states. The
//! report has no timestamps, so the same input gives the same report.

pub mod host;
pub mod manifest;

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use tpt_commercial_cli::exit;
use tpt_primitives::computation::artifact_of_content;
use tpt_runtime_core::WorkloadState;
use tpt_wasm_runtime::{Config, Engine, RuntimeError};
use tpt_wasm_types::{ResourceLimits, Trap, Value};

use host::{Import, Op, Shared, MODULE};
use manifest::{resolve, Limits, Manifest};

pub const PRODUCT: &str = "tpt-secure-run";
pub const RUNNER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The export every script must provide: `run() -> i32`. Zero means success.
pub const ENTRY: &str = "run";

/// How a run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// The script returned. Its status is in `script_status`.
    Completed,
    /// The script imports something the manifest does not grant. It never ran.
    Denied,
    /// The module is not valid, or has no `run` export. It never ran.
    InvalidModule,
    /// The script trapped, for example on an out-of-bounds access.
    Trapped,
    /// The script used up its step, memory or call-depth limit.
    LimitExceeded,
    /// The script finished, but its output could not be written.
    OutputFailed,
}

/// The result of one run. Serialized as the JSON report.
#[derive(Debug, Serialize)]
pub struct Report {
    pub product: &'static str,
    pub version: &'static str,
    pub script: String,
    /// Content-addressed identity of the module (`tpt-primitives` ArtifactId).
    pub module_id: String,
    pub module_bytes: usize,
    pub lifecycle: Vec<String>,
    pub status: Status,
    pub script_status: Option<i32>,
    pub reason: Option<String>,
    pub limits: Limits,
    /// Host imports the manifest grants, so the script could link them.
    pub granted_imports: Vec<&'static str>,
    pub log: Vec<i64>,
    pub denials: Vec<String>,
    pub read_bytes: u64,
    pub write_bytes: u64,
    /// Names of the output grants that were written.
    pub outputs_written: Vec<String>,
}

impl Report {
    /// The process exit code for this run. See the README for the table.
    pub fn exit_code(&self) -> u8 {
        match self.status {
            Status::Completed if self.script_status == Some(0) => exit::SUCCESS,
            Status::Completed => codes::SCRIPT_NONZERO,
            Status::Denied => codes::DENIED,
            Status::InvalidModule => codes::INVALID_MODULE,
            Status::Trapped | Status::LimitExceeded => codes::SCRIPT_FAILED,
            Status::OutputFailed => exit::IO_ERROR,
        }
    }
}

/// Exit codes for `run`. 0 and 1 are shared with the other products (`tpt-commercial-cli`).
pub mod codes {
    /// The module is not valid, or has no `run` export.
    pub const INVALID_MODULE: u8 = 3;
    /// The script trapped or used up a limit.
    pub const SCRIPT_FAILED: u8 = 4;
    /// The script returned a non-zero status.
    pub const SCRIPT_NONZERO: u8 = 6;
    /// The script imports something the manifest does not grant.
    pub const DENIED: u8 = 7;
}

/// Tracks the lifecycle path, checking every step against the runtime's table.
struct Lifecycle {
    state: WorkloadState,
    path: Vec<WorkloadState>,
}

impl Lifecycle {
    fn new() -> Self {
        Self {
            state: WorkloadState::Defined,
            path: vec![WorkloadState::Defined],
        }
    }

    fn to(&mut self, next: WorkloadState) {
        let state = self
            .state
            .transition(next)
            .unwrap_or_else(|| panic!("lifecycle: {} -> {next} is not permitted", self.state));
        self.state = state;
        self.path.push(state);
    }

    fn names(&self) -> Vec<String> {
        self.path.iter().map(|s| s.to_string()).collect()
    }
}

/// Run a script. `base_dir` is the folder that relative manifest paths are taken from.
pub fn run_module(manifest: &Manifest, base_dir: &Path, wasm: &[u8]) -> Report {
    let mut lc = Lifecycle::new();
    lc.to(WorkloadState::Resolved);

    let reads: Vec<PathBuf> = manifest
        .resources
        .fs_read
        .iter()
        .map(|g| resolve(base_dir, &g.path))
        .collect();
    let writes: Vec<PathBuf> = manifest
        .resources
        .fs_write
        .iter()
        .map(|g| resolve(base_dir, &g.path))
        .collect();
    let shared = Shared::new(reads, writes);
    lc.to(WorkloadState::Prepared);

    let mut report = Report {
        product: PRODUCT,
        version: RUNNER_VERSION,
        script: manifest.name.clone(),
        module_id: artifact_of_content(wasm).to_string(),
        module_bytes: wasm.len(),
        lifecycle: Vec::new(),
        status: Status::InvalidModule,
        script_status: None,
        reason: None,
        limits: manifest.limits,
        granted_imports: shared.offered().iter().map(|op| op.name()).collect(),
        log: Vec::new(),
        denials: Vec::new(),
        read_bytes: 0,
        write_bytes: 0,
        outputs_written: Vec::new(),
    };

    let module = match tpt_wasm_decode::decode(wasm) {
        Ok(module) => module,
        Err(e) => return finish_early(report, lc, Status::InvalidModule, format!("{e:?}")),
    };

    let denials = check_imports(&module, &shared.offered());
    if let Some(first) = denials.first().cloned() {
        report.denials = denials;
        return finish_early(report, lc, Status::Denied, first);
    }

    let engine_config = Config {
        deterministic: true,
        limits: ResourceLimits {
            max_memory_pages: manifest.limits.max_memory_pages,
            max_call_depth: manifest.limits.max_call_depth,
            max_execution_steps: Some(manifest.limits.max_steps),
            ..ResourceLimits::default()
        },
        ..Config::default()
    };
    let mut engine = match Engine::new(engine_config) {
        Ok(engine) => engine,
        Err(e) => return finish_early(report, lc, Status::InvalidModule, e.to_string()),
    };
    for op in shared.offered() {
        engine
            .linker_mut()
            .define_function(MODULE, op.name(), Import::new(&shared, op));
    }

    let mut instance = match engine.instantiate(module) {
        Ok(instance) => instance,
        Err(e) => return finish_early(report, lc, Status::InvalidModule, e.to_string()),
    };
    if instance.get_function(ENTRY).is_err() {
        let reason = format!("the module has no exported function '{ENTRY}'");
        return finish_early(report, lc, Status::InvalidModule, reason);
    }
    // Imports are linked and checked by here. Nothing has run yet.
    lc.to(WorkloadState::Created);
    lc.to(WorkloadState::Starting);
    lc.to(WorkloadState::Running);

    let outcome = instance.call(ENTRY, Vec::new());
    let (status, script_status, reason) = match outcome {
        Ok(values) => match values.as_slice() {
            [Value::I32(code)] => (Status::Completed, Some(*code), None),
            _ => (
                Status::InvalidModule,
                None,
                Some(format!("'{ENTRY}' must return one i32")),
            ),
        },
        Err(RuntimeError::Trap(trap)) => {
            let status = match trap {
                Trap::StepsExhausted | Trap::CallDepthExceeded | Trap::StackOverflow => {
                    Status::LimitExceeded
                }
                _ => Status::Trapped,
            };
            (status, None, Some(trap.to_string()))
        }
        Err(e) => (Status::Trapped, None, Some(e.to_string())),
    };

    let collected = shared.collect();
    report.log = collected.log;
    report.denials = collected.denials;
    report.read_bytes = collected.read_bytes;
    report.write_bytes = collected.write_bytes;
    report.status = status;
    report.script_status = script_status;
    report.reason = reason;

    if status != Status::Completed {
        lc.to(WorkloadState::Failed);
        report.lifecycle = lc.names();
        return report;
    }
    lc.to(WorkloadState::Stopping);
    lc.to(WorkloadState::Stopped);
    report.lifecycle = lc.names();

    // Outputs are written only when the script reports success, so a failed run leaves no partial file.
    if script_status == Some(0) {
        if let Err((name, e)) = write_outputs(manifest, base_dir, &collected.outputs, &mut report) {
            report.status = Status::OutputFailed;
            report.reason = Some(format!("could not write output '{name}': {e}"));
        }
    }
    report
}

/// End a run before the script starts: the lifecycle goes to `failed`.
fn finish_early(mut report: Report, mut lc: Lifecycle, status: Status, reason: String) -> Report {
    report.status = status;
    report.reason = Some(reason);
    lc.to(WorkloadState::Failed);
    report.lifecycle = lc.names();
    report
}

/// Every import the module needs that the runner does not offer, as denial reasons.
fn check_imports(module: &tpt_wasm_format::Module, offered: &[Op]) -> Vec<String> {
    use tpt_wasm_format::ImportDesc;
    let mut denials = Vec::new();
    for import in &module.imports {
        let qualified = format!("{}.{}", import.module, import.name);
        if import.module != MODULE {
            denials.push(format!(
                "imports {qualified}, but only '{MODULE}' host functions exist"
            ));
            continue;
        }
        if !matches!(import.desc, ImportDesc::Function(_)) {
            denials.push(format!(
                "imports {qualified} as a memory, table or global, which the runner does not offer"
            ));
            continue;
        }
        match Op::ALL.iter().find(|op| op.name() == import.name) {
            None => denials.push(format!("imports {qualified}, which is not a host function")),
            Some(op) if !offered.contains(op) => {
                let grant = op.grant().unwrap_or("none");
                denials.push(format!(
                    "imports {qualified}, which needs the {grant} grant; the manifest does not give it"
                ));
            }
            Some(_) => {}
        }
    }
    denials
}

fn write_outputs(
    manifest: &Manifest,
    base_dir: &Path,
    outputs: &std::collections::BTreeMap<usize, Vec<u8>>,
    report: &mut Report,
) -> Result<(), (String, std::io::Error)> {
    for (handle, bytes) in outputs {
        let grant = &manifest.resources.fs_write[*handle];
        let path = resolve(base_dir, &grant.path);
        fs::write(&path, bytes).map_err(|e| (grant.name.clone(), e))?;
        report.outputs_written.push(grant.name.clone());
    }
    Ok(())
}

/// Check a module against a manifest without running it. Returns the reasons it would be refused.
pub fn check_module(manifest: &Manifest, wasm: &[u8]) -> Result<Vec<String>, String> {
    let module = tpt_wasm_decode::decode(wasm).map_err(|e| format!("{e:?}"))?;
    let shared = Shared::new(
        manifest
            .resources
            .fs_read
            .iter()
            .map(|g| g.path.clone())
            .collect(),
        manifest
            .resources
            .fs_write
            .iter()
            .map(|g| g.path.clone())
            .collect(),
    );
    Ok(check_imports(&module, &shared.offered()))
}

/// The runner's built-in checks, used by `doctor`. Returns the failed checks.
pub fn self_test() -> Vec<String> {
    let mut failures = Vec::new();
    let empty = Manifest {
        name: "self-test".into(),
        resources: Default::default(),
        limits: Limits::default(),
    };

    // A pure script runs and returns its value.
    // (module (func (export "run") (result i32) i32.const 2 i32.const 3 i32.add))
    let pure = [
        0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, // header
        0x01, 0x05, 0x01, 0x60, 0x00, 0x01, 0x7f, // type () -> i32
        0x03, 0x02, 0x01, 0x00, // function 0 has type 0
        0x07, 0x07, 0x01, 0x03, 0x72, 0x75, 0x6e, 0x00, 0x00, // export "run"
        0x0a, 0x09, 0x01, 0x07, 0x00, 0x41, 0x02, 0x41, 0x03, 0x6a, 0x0b, // body
    ];
    let report = run_module(&empty, Path::new("."), &pure);
    if report.status != Status::Completed || report.script_status != Some(5) {
        failures.push(format!(
            "pure script: expected completed with 5, got {:?}",
            report.status
        ));
    }

    // A script that imports a file it was not granted is refused before it runs.
    let denied = [
        0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x01, 0x06, 0x01, 0x60, 0x01, 0x7f, 0x01,
        0x7f, 0x02, 0x14, 0x01, 0x03, 0x74, 0x70, 0x74, 0x0c, 0x66, 0x73, 0x5f, 0x6f, 0x70, 0x65,
        0x6e, 0x5f, 0x72, 0x65, 0x61, 0x64, 0x00, 0x00,
    ];
    let report = run_module(&empty, Path::new("."), &denied);
    if report.status != Status::Denied {
        failures.push(format!(
            "ungranted import: expected denied, got {:?}",
            report.status
        ));
    }
    failures
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn self_test_passes() {
        assert_eq!(self_test(), Vec::<String>::new());
    }
}
