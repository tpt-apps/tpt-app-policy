# TPT Secure Script Runner

`tpt-secure-run` runs a WebAssembly script with only the file access that its
manifest grants. A script has no authority by default. It can reach files only
through host functions, and each host function is available only if the manifest
grants it. Anything else is refused before the script runs.

It runs offline and sends nothing anywhere.

## Quick start

```
tpt-secure-run validate examples/secure-run/manifest.yaml --module examples/secure-run/count-lines.wasm
tpt-secure-run run examples/secure-run/manifest.yaml examples/secure-run/count-lines.wasm
```

The second command prints a JSON report, and writes `out.txt` next to the manifest.
`out.txt` is a copy of `input.txt`. The report's `log` field holds the line count.

To see a denial:

```
tpt-secure-run run examples/secure-run/no-grants.yaml examples/secure-run/count-lines.wasm
```

It exits with code 7 and says which grant is missing. The script never runs.

## The manifest

```yaml
name: count-lines          # shown in the report
resources:
  fs_read:                 # files the script may read
    - name: input
      path: input.txt      # relative paths are taken from the manifest's folder
  fs_write:                # files the script may write
    - name: copy
      path: out.txt
limits:
  max_steps: 10000000      # instructions before the script is stopped
  max_memory_pages: 16     # 64 KiB pages; 16 is 1 MiB
  max_call_depth: 256
```

Rules:

- Unknown keys are an error. A typo cannot quietly grant or drop anything.
- `net_connect`, `process_spawn` and `env` are recognised and refused. They are not
  supported in this version, so the runner fails closed rather than ignoring them.
- Grant names must be unique. Each grant must have a name and a path.
- Files are capped at 10 MiB, each way.

## What a script sees

A script is a WebAssembly module. It must export `run` with the signature `() -> i32`.
The returned value is its status: 0 means success.

The runner offers these host functions, in the module `tpt`:

| Import | Grant needed | Meaning |
|---|---|---|
| `log(i64)` | none | Records a value in the report's `log`. |
| `fs_open_read(handle: i32) -> i32` | `fs_read` | Reads granted file `handle` into memory. Returns a descriptor, or -1. |
| `fs_size(fd: i32) -> i64` | `fs_read` | File length in bytes, or -1. |
| `fs_byte(fd: i32, offset: i64) -> i32` | `fs_read` | One byte, or -1. |
| `fs_write_byte(handle: i32, byte: i32) -> i32` | `fs_write` | Appends a byte to granted output `handle`. Returns 0, or -1. |

A handle is a file's position in its manifest list (0, 1, ...). The script never
supplies a path. The path comes only from the manifest.

The interface passes integers only. A script cannot yet pass strings to the host,
so it works on bytes. That is a limit of this version.

Output is written to disk only if the script returns 0. A failed or cancelled run
leaves no partial file.

## Denials and exit codes

A script is refused before it runs if it imports:

- a module other than `tpt`, such as WASI, or any other host;
- a `tpt` function that does not exist;
- a `tpt` function whose grant is missing;
- a memory, table or global from the host.

| Exit code | Meaning |
|---|---|
| 0 | The script returned 0 (`validate`: the manifest is valid and no import is denied) |
| 1 | A file could not be read or written |
| 2 | Invalid manifest, or a bad command line |
| 3 | The module is not valid WebAssembly, or has no `run` export |
| 4 | The script trapped, or used up its step, memory or call-depth limit |
| 5 | `doctor` found a problem |
| 6 | The script returned a non-zero status |
| 7 | The script imports something the manifest does not grant. It never ran. |

## Report

The report is JSON with no timestamps, so the same input always gives the same report.
It includes:

- `module_id`: a content-addressed identity of the module (SHA-256 of its bytes, using
  `tpt-primitives`' ArtifactId). Two runs of the same bytes give the same ID.
- `lifecycle`: the path through the runtime's workload states, from `defined` to
  `stopped`, or to `failed`. Each step is checked against the runtime's transition table.
- `status`, `script_status`, `reason`.
- `limits`, `granted_imports`, `log`, `denials`.
- `read_bytes`, `write_bytes`, `outputs_written`.

## Security model

Read this before relying on the runner for anything sensitive.

- **Default deny.** Authority comes only from the manifest. Nothing else is linked.
- **Checked before running.** Every import is checked against the grants before the
  script starts. A denied script never executes.
- **No paths from the script.** A script names files by handle, so it cannot reach a
  path the manifest did not list.
- **Scoped tokens.** Each file read goes through a `tpt-capsec` token minted from the
  run's root capability and scoped to the granted path.
- **Bounded resources.** Steps, memory, call depth, file size, open files and log
  length are all capped. Limits are enforced by the engine, not by polling.
- **Deterministic engine.** The `tpt-wasm` engine runs in deterministic mode, with no
  clock or randomness.

What this does not give you:

- **It is not a process sandbox.** The script runs inside the runner's process. The
  guarantee comes from the WebAssembly boundary and the host functions. A bug in the
  host functions, or in the engine, would weaken it. A hardened process boundary
  (tpt-archon or tpt-nexus) is deferred until native subprocess support exists.
- **The capability tokens are a convention, not an OS barrier.** `tpt-capsec` checks
  scope in the code that holds the token. The host calls `std::fs` directly, so a
  future change that bypassed the token would not be caught by the compiler.
- **No network, environment or subprocess access.** These are refused, not sandboxed.
- **Not reviewed by an outside party.** Test it against your own threat model before
  you rely on it.

## Building scripts

The runner accepts only WebAssembly binaries (`.wasm`). Compile your script with any
toolchain that targets WebAssembly. The examples ship as WebAssembly text (`.wat`) and
binary (`.wasm`). To rebuild a binary from its text:

```
cargo run -p tpt-secure-run --example compile_wat -- examples/secure-run/count-lines.wat examples/secure-run/count-lines.wasm
```

## Limits in this version

- Scripts work on bytes only. No strings, and no structured data passed to the host.
- Files are read whole, up to 10 MiB, so a script cannot stream a larger file.
- One script per run. The runner has no daemon or scheduler.
- Network, environment and subprocess grants are not supported.

## Related

- [Permission manifest reference](#the-manifest) (this document)
- [AI Action Guard](AI_ACTION_GUARD.md): decides whether an AI agent's action may run.
  The runner executes a script that has already been allowed.
- [SECURITY](SECURITY.md)
