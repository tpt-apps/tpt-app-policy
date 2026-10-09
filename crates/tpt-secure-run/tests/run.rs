use std::fs;
use std::path::{Path, PathBuf};

use tpt_secure_run::manifest::parse_manifest;
use tpt_secure_run::{run_module, Report, Status};

/// A fresh folder for one test, holding `input.txt` with the given bytes.
fn scratch(name: &str, input: &[u8]) -> PathBuf {
    let dir = std::env::temp_dir()
        .join("tpt-secure-run-tests")
        .join(format!("{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("input.txt"), input).unwrap();
    dir
}

fn run(manifest: &str, dir: &Path, wat_src: &str) -> Report {
    let manifest = parse_manifest(manifest).unwrap();
    let wasm = wat::parse_str(wat_src).unwrap();
    run_module(&manifest, dir, &wasm)
}

const READ_ONE: &str =
    "name: reader\nresources:\n  fs_read:\n    - {name: input, path: input.txt}\n";

#[test]
fn pure_script_runs_and_its_log_is_kept() {
    let dir = scratch("log", b"");
    let r = run(
        "name: logger\n",
        &dir,
        r#"(module
            (import "tpt" "log" (func $log (param i64)))
            (func (export "run") (result i32)
                i64.const 42
                call $log
                i32.const 0))"#,
    );
    assert_eq!(r.status, Status::Completed);
    assert_eq!(r.script_status, Some(0));
    assert_eq!(r.log, vec![42]);
    assert_eq!(r.exit_code(), 0);
    assert_eq!(r.denials, Vec::<String>::new());
}

#[test]
fn granted_file_can_be_read_byte_by_byte() {
    let dir = scratch("read", b"A");
    // Returns the first byte of input.txt, which is 'A' (65).
    let r = run(
        READ_ONE,
        &dir,
        r#"(module
            (import "tpt" "fs_open_read" (func $open (param i32) (result i32)))
            (import "tpt" "fs_byte" (func $byte (param i32 i64) (result i32)))
            (func (export "run") (result i32)
                i32.const 0
                call $open
                i64.const 0
                call $byte))"#,
    );
    assert_eq!(r.status, Status::Completed);
    assert_eq!(r.script_status, Some(65));
    assert_eq!(r.read_bytes, 1);
    assert_eq!(r.exit_code(), tpt_secure_run::codes::SCRIPT_NONZERO);
}

#[test]
fn output_is_written_only_when_the_script_returns_zero() {
    let dir = scratch("write-ok", b"");
    let manifest = "name: writer\nresources:\n  fs_write:\n    - {name: result, path: out.txt}\n";
    let wat = r#"(module
        (import "tpt" "fs_write_byte" (func $w (param i32 i32) (result i32)))
        (func (export "run") (result i32)
            i32.const 0 i32.const 104 call $w drop
            i32.const 0 i32.const 105 call $w drop
            i32.const 0))"#;
    let r = run(manifest, &dir, wat);
    assert_eq!(r.status, Status::Completed);
    assert_eq!(r.outputs_written, vec!["result".to_string()]);
    assert_eq!(fs::read(dir.join("out.txt")).unwrap(), b"hi");
}

#[test]
fn failed_script_leaves_no_partial_output() {
    let dir = scratch("write-fail", b"");
    let manifest = "name: writer\nresources:\n  fs_write:\n    - {name: result, path: out.txt}\n";
    let wat = r#"(module
        (import "tpt" "fs_write_byte" (func $w (param i32 i32) (result i32)))
        (func (export "run") (result i32)
            i32.const 0 i32.const 104 call $w drop
            i32.const 3))"#;
    let r = run(manifest, &dir, wat);
    assert_eq!(r.script_status, Some(3));
    assert_eq!(r.exit_code(), tpt_secure_run::codes::SCRIPT_NONZERO);
    assert!(!dir.join("out.txt").exists());
    assert!(r.outputs_written.is_empty());
}

#[test]
fn import_without_its_grant_is_denied_before_the_script_runs() {
    let dir = scratch("denied-read", b"A");
    let r = run(
        "name: no-grants\n",
        &dir,
        r#"(module
            (import "tpt" "fs_open_read" (func $open (param i32) (result i32)))
            (func (export "run") (result i32)
                i32.const 0
                call $open))"#,
    );
    assert_eq!(r.status, Status::Denied);
    assert_eq!(r.exit_code(), tpt_secure_run::codes::DENIED);
    assert!(
        r.reason.as_deref().unwrap().contains("fs_read grant"),
        "{:?}",
        r.reason
    );
    assert_eq!(r.read_bytes, 0);
    assert_eq!(r.lifecycle.last().map(String::as_str), Some("failed"));
}

#[test]
fn network_and_unknown_imports_are_denied() {
    let dir = scratch("denied-net", b"");
    for import in [
        r#"(import "tpt" "net_connect" (func (param i32) (result i32)))"#,
        r#"(import "wasi_snapshot_preview1" "fd_write" (func (param i32) (result i32)))"#,
        r#"(import "tpt" "make_coffee" (func))"#,
    ] {
        let r = run(
            "name: x\n",
            &dir,
            &format!("(module {import} (func (export \"run\") (result i32) i32.const 0))"),
        );
        assert_eq!(r.status, Status::Denied, "{import}");
    }
}

#[test]
fn out_of_range_handle_is_refused_and_recorded() {
    let dir = scratch("handle", b"A");
    let r = run(
        READ_ONE,
        &dir,
        r#"(module
            (import "tpt" "fs_open_read" (func $open (param i32) (result i32)))
            (func (export "run") (result i32)
                i32.const 5
                call $open))"#,
    );
    assert_eq!(r.script_status, Some(-1));
    assert!(
        r.denials
            .iter()
            .any(|d| d.contains("handle 5 is not granted")),
        "{:?}",
        r.denials
    );
}

#[test]
fn endless_loop_stops_at_the_step_limit() {
    let dir = scratch("steps", b"");
    let r = run(
        "name: spin\nlimits:\n  max_steps: 1000\n",
        &dir,
        r#"(module (func (export "run") (result i32)
            (loop $l br $l)
            i32.const 0))"#,
    );
    assert_eq!(r.status, Status::LimitExceeded);
    assert_eq!(r.exit_code(), tpt_secure_run::codes::SCRIPT_FAILED);
}

#[test]
fn memory_above_the_manifest_limit_is_refused_as_invalid() {
    let dir = scratch("memory", b"");
    let r = run(
        "name: big\nlimits:\n  max_memory_pages: 2\n",
        &dir,
        r#"(module (memory 64) (func (export "run") (result i32) i32.const 0))"#,
    );
    assert_eq!(r.status, Status::InvalidModule);
    assert_eq!(r.exit_code(), tpt_secure_run::codes::INVALID_MODULE);
}

#[test]
fn missing_run_export_is_invalid() {
    let dir = scratch("no-run", b"");
    let r = run(
        "name: x\n",
        &dir,
        r#"(module (func (export "main") (result i32) i32.const 0))"#,
    );
    assert_eq!(r.status, Status::InvalidModule);
    assert!(r.reason.unwrap().contains("'run'"));
}

#[test]
fn bytes_that_are_not_wasm_are_invalid() {
    let dir = scratch("not-wasm", b"");
    let manifest = parse_manifest("name: x\n").unwrap();
    let r = run_module(&manifest, &dir, b"#!/bin/sh\necho hi\n");
    assert_eq!(r.status, Status::InvalidModule);
    assert_eq!(r.exit_code(), tpt_secure_run::codes::INVALID_MODULE);
}

#[test]
fn trap_is_reported_not_panicked() {
    let dir = scratch("trap", b"");
    let r = run(
        "name: x\n",
        &dir,
        r#"(module (func (export "run") (result i32) unreachable))"#,
    );
    assert_eq!(r.status, Status::Trapped);
    assert_eq!(r.exit_code(), tpt_secure_run::codes::SCRIPT_FAILED);
}

#[test]
fn lifecycle_follows_the_runtime_states() {
    let dir = scratch("lifecycle", b"");
    let r = run(
        "name: x\n",
        &dir,
        r#"(module (func (export "run") (result i32) i32.const 0))"#,
    );
    assert_eq!(
        r.lifecycle,
        [
            "defined", "resolved", "prepared", "created", "starting", "running", "stopping",
            "stopped"
        ]
    );
}

#[test]
fn same_input_gives_the_same_report() {
    let dir = scratch("determinism", b"A");
    let wat = r#"(module
        (import "tpt" "log" (func $log (param i64)))
        (import "tpt" "fs_open_read" (func $open (param i32) (result i32)))
        (func (export "run") (result i32)
            i64.const 9 call $log
            i32.const 0 call $open drop
            i32.const 0))"#;
    let a = serde_json::to_string(&run(READ_ONE, &dir, wat)).unwrap();
    let b = serde_json::to_string(&run(READ_ONE, &dir, wat)).unwrap();
    assert_eq!(a, b);
}
