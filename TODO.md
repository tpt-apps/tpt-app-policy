# TPT Commercial Portfolio — TODO

Source: `spec.txt` (section refs in parentheses). Tick boxes as you go: `- [x]`.

## Decisions locked in

- Scope: full 10-phase portfolio, built in order (§41). Don't build products in parallel (§50).
- Layout: single Cargo workspace in this folder; split into separate repos later if wanted (§34 names kept as crate names).
- Existing TPT repos: github.com/TPT-Solutions/* (tpt-primitives, tpt-capsec, tpt-wasm, tpt-runtime, tpt-crypto, tpt-mcpbox).
- Licensing: dual MIT OR Apache-2.0 (open source). Still sold on Gumroad (see `docs/GUMROAD.md`). No keys, no activation, no phone-home (§26).
- Conflict resolution: severity precedence `rejected > approval_required > review > approved`; approvers/requirements merged from all matched rules (§48 demo).
- CLI: per-product binaries on a shared framework (`tpt-policy`, `tpt-data`, ...). Each has its own `doctor`.
- REST: `serve` subcommand inside `tpt-policy`, `POST /v1/evaluate`, localhost by default.
- Platforms: Windows x64 + Linux x64 only at launch (§24).

## Open decisions (default shown; change if wrong)

- [ ] Rust toolchain / MSRV — default: latest stable, pin in `rust-toolchain.toml`
- [x] Combinator syntax for AND/OR/NOT — implemented: `all:` / `any:` / `not:`; sibling fields are AND-ed
- [x] Operator set — implemented: `equals, not_equals, gt, gte, lt, lte, in, not_in, contains, exists`. `matches` (regex) deferred: no regex dependency yet
- [x] Dotted paths and nested YAML — implemented; numeric segments index lists
- [x] Missing field behaviour — implemented: any check on a missing or null field fails, except `exists: false`
- [x] Decision when no rule matches — implemented: optional `default_decision`, defaults to `review` (fails safe; the spec gives no default)
- [x] Exit-code table — implemented: 0 approved/success, 10 approval_required, 20 review, 30 rejected, 1 file read, 2 invalid policy or usage, 3 invalid input JSON, 4 test failed. Documented in README.md
- [x] REST server crate — changed from the default axum to `tiny_http` (synchronous, no async runtime, fewer dependencies). Bearer token optional, read from `TPT_POLICY_TOKEN`
- [x] Which crates to reuse from TPT-Solutions vs. write fresh — decided: `tpt-policy-core` stays independent of `tpt-runtime-policy` and `tpt-mcpbox-policy`. Keeps the commercial core small and stable (§33).
- [x] Product pricing: one product, one price, one sale (suggested $49). No tiers, no update promise. Listing in `docs/GUMROAD.md`

## Phase 0 — Workspace & shared groundwork

- [x] Init git repo, `.gitignore`, `rust-toolchain.toml` (repo initialised, nothing committed)
- [x] Create Cargo workspace root
- [x] Create crates: `tpt-policy-core`, `tpt-schema`, `tpt-report`, `tpt-commercial-cli`, `tpt-app-policy`
- [x] CI (build + test + clippy + fmt) for Windows and Linux (workflow written in `.github/workflows/ci.yml`; not yet run on GitHub)
- [x] Clone/audit TPT-Solutions repos: tpt-primitives, tpt-capsec, tpt-wasm, tpt-runtime, tpt-crypto, tpt-mcpbox (findings in `docs/ARCHITECTURE.md`)
- [ ] Define the narrow stable dependency layer (§33); record API gaps (draft in ARCHITECTURE.md; needs your decision on tpt-runtime-policy overlap)
- [x] Pick workspace dependencies (serde, serde_yaml, serde_json, clap, thiserror, ...) (declared in `Cargo.toml`, used from Phase 1)
- [x] Write `docs/ARCHITECTURE.md` (JSON boundary, layering, what NOT to build §42)

## Phase 1 — Policy Foundation → TPT App Policy 0.1

Status: the core of 0.1 is built and tested (35 tests, clippy clean). Items left open below are marked with what is missing.

### tpt-policy-core (§7, §34, §35) — done
- [x] Rule AST (`src/model.rs`)
- [x] YAML parser with validation (`src/parse.rs`). Syntax errors give line and column. Semantic errors give the key path (e.g. `rules[0].when.amount.greater_than`), not a line number. Line numbers for semantic errors are a possible follow-up.
- [x] Policy metadata: `policy`, `version` (optional, recorded as `unversioned` when missing), rule `id`, `description`
- [x] Field-path resolution (nested objects, list indexes)
- [x] Operators: equality, inequality, gt/gte/lt/lte, string compare, list membership, existence, contains
- [x] Types: strings, numbers (int and float compare by value), booleans, arrays, nested objects
- [x] AND / OR / NOT conditions
- [x] Decision model + severity precedence merge
- [x] Merged outputs: approvers, requirements, warnings (`requirement` and `warning` on a rule; `approver` as text or `{role: ...}`)
- [x] Explanation model: matched rules with descriptions, failed rules with the first failed check
- [~] Policy versioning: version is recorded in every output. **Not done:** `name@x.y.z` form, and checking it against the app version (§37)
- [x] Determinism: no clock or randomness in evaluation; repeated runs are tested to be byte-identical
- [x] Validation errors with what / where / why / fix (§29.4)
- [x] Inline `tests:` block (name / input / expect), with optional `matched_rules` (§36)
- [x] Tests: 24 behaviour tests in `crates/tpt-policy-core/tests/evaluation.rs`

### tpt-schema (minimal at this stage) (§34)
- [x] Schema abstraction + validation model + diagnostics. Done in Phase 3: `parse_schema` (what/where/why/fix errors), `check_record` (types, required, enum, pattern, min/max), `UniqueIndex`. Tested in `crates/tpt-schema/tests/schema.rs`.

### tpt-report (§20)
- [~] Common report model (status, timestamp, product, version, input, summary, errors). **Partly done:** the reporters take `Evaluation` from `tpt-policy-core`. No separate report model yet, and the JSON output has no product or timestamp fields by design.
- [x] JSON reporter
- [x] Terminal reporter. Moved from the binary into `tpt-report::terminal`.
- [ ] HTML reporter (can wait until Phase 3)

### tpt-commercial-cli (§18, §19, §38)
- [~] Common flags. **Done:** `--format` on `check` and `explain`, `--version`, `--output FILE` on `check`/`explain`/`run`, `--quiet` on `validate`/`test`. **Missing:** `--config --policy --schema --json --verbose` (`--config` and `--schema` wait on their designs; `--json` and `--verbose` are not needed by any current command).
- [ ] `--debug`, `--json-logs`; logs never dump customer records by default. **Not started.** Current output contains no logs.
- [ ] Config discovery (`./tpt/config|policies|schemas|templates|reports`), no hidden DB. **Not started.**
- [ ] Diagnostics formatter. **Partly done:** `PolicyError` renders what / where / why / fix; not yet shared across products
- [x] Stable, documented exit codes (`crates/tpt-commercial-cli/src/lib.rs`, table in README.md)

### tpt-app-policy binary `tpt-policy` (§8, §47–48)
- [x] `validate policy.yaml`
- [x] `check policy.yaml input.json`
- [x] `explain policy.yaml input.json`
- [x] `test policy.yaml` (PASS/FAIL output and summary)
- [x] `run policy.yaml` via stdin/stdout
- [x] `--version`
- [~] Audit fields in output (§21). **Done:** engine version, policy name and version, rule IDs, and `input_sha256` (canonical SHA-256 of the input). **Missing:** a separate product version (the engine and the product are one crate, so `engine_version` serves for now), schema version (no schema yet). No timestamp, on purpose, so output stays deterministic.
- [x] §48 expense demo: the output has the spec's `decision`, `approvers` and `matched_rules`. It also has extra fields (explanations, failed rules, warnings, versions).
- [x] README draft with quickstart (`README.md`). Separate QUICKSTART file not yet split out.
- [ ] Tag **App Policy 0.1**. Waiting on your go-ahead to commit and tag. Nothing is committed yet.

### Spec gaps found while building Phase 1
- §35's example has no rule for amounts of $5,000 and above, so those fall through to the default. `examples/purchasing/` adds a `purchase-director` rule and says so in a comment.
- §7.2 and §48 omit the policy version. It is optional here and defaults to `unversioned`.
- §14 writes `approver: {role: manager}`. Both that and plain `approver: manager` are accepted.

## Phase 2 — Production Runtime → TPT App Policy 1.0

- [ ] Integrate tpt-primitives (types/IDs/deterministic representations)
- [ ] Integrate tpt-capsec (capability model)
- [ ] Integrate tpt-runtime (lifecycle, resource limits)
- [~] Integrate tpt-wasm (WASM sandbox). **Checked:** tpt-wasm is MIT OR Apache-2.0. Its only third-party dependency is `wast` (Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT), used only by `tpt-wasm-spec`, a spec test harness. So a product that does not depend on `tpt-wasm-spec` picks up no third-party licence issue. Re-run `cargo deny` on the final dependency tree before embedding.
- [~] Resource limits (time, memory, input size). **Done:** input size: 10 MB in the CLI, 1 MB over HTTP. **Not needed yet:** time limits, since evaluation is linear in rules and input size and has no loops or I/O. Memory follows from the input size limit.
- [x] Deterministic mode (always on: no clock, no randomness; tested byte-identical across runs)
- [ ] Expose policy evaluation as an embeddable WASM module (§8.4)
- [x] REST: `tpt-policy serve`, `POST /v1/evaluate` (plus `GET /healthz`). Localhost by default, optional bearer token, 1 MB body limit. Tested over real sockets. Single-threaded.
- [x] `doctor` subcommand: runtime, config, permissions, install, version, optional deps (§29.5). Checks version, platform, working-directory write test, `./tpt` layout (§19, optional), and a built-in self-test. Optional-dependency checks are not needed yet, since no optional dependencies exist.
- [ ] Signed policy bundles / release checksums via tpt-crypto (optional signatures)
- [x] Docker image (`tpt/app-policy`) (§25). Built and tested on Docker Desktop (Windows): `doctor`, `check` (matches the golden output), `serve` with healthz and bearer auth, non-root user. Image is 117 MB. Linux and macOS not yet tested.
- [x] Release bundle layout (§23). `scripts/package.sh` builds it, with checksums. Tested on Windows (Git Bash) and Linux (in a rust:1-bookworm container). macOS not yet tested. It refuses to package without `LICENSE-MIT` and `LICENSE-APACHE`.
- [ ] Windows x64 build + install test on a clean machine
- [~] Linux x64 build + install test. **Done:** fmt, clippy, all 126 tests, and the four bundles built and unpacked in a `rust:1-bookworm` container, with `--version` and `doctor` run from each bundle. **Not done:** a clean machine, with no Rust toolchain installed.
- [~] Checksums + release notes for each version (§40). **Done:** `SHA256SUMS` from the packaging script, and `CHANGELOG.md`. **Missing:** signatures (optional in §40)

### Docs for 1.0 (§30)
- [x] README
- [x] INSTALL (in `docs/`)
- [x] QUICKSTART (in `docs/`)
- [x] CONCEPTS (in `docs/`)
- [ ] CONFIGURATION (blocked: no config file format exists yet, so there is nothing to document)
- [x] CLI_REFERENCE (in `docs/`)
- [x] POLICY_REFERENCE (in `docs/`)
- [x] SCHEMA_REFERENCE (in `docs/`)
- [~] INTEGRATION (in `docs/`). **Done:** generic JSON-in/decision-out pattern with HTTP and CLI examples. **Missing:** a real ERP example, which needs a chosen ERP and sample exports.
- [x] TROUBLESHOOTING (in `docs/`)
- [x] SECURITY (in `docs/`)
- [x] LICENCE: dual MIT OR Apache-2.0 (`LICENSE-MIT`, `LICENSE-APACHE`). Copyright holder line in `LICENSE-MIT` to confirm.
- [~] CHANGELOG (Unreleased section exists; needs a tagged version)

### Commercial wrapper
- [x] Licence (§26) replaced by dual MIT OR Apache-2.0. A separate commercial licence is no longer planned; the paid offer is the Gumroad product (see `docs/GUMROAD.md`).
- [ ] Support-boundary statement (§43)
- [ ] Privacy statement: no data leaves the machine (§39)
- [ ] Landing page (§31)
- [ ] Gumroad listing: one product, $49 suggested (§27). Full setup in `docs/GUMROAD.md`. Still to do: set the price in Gumroad, the refund wording, and the checklist there.
- [ ] Policy templates/examples (purchase order, expense, etc.)

### 1.0 acceptance criteria (§47) — do not ship until all pass
- [ ] Windows install works
- [ ] Linux install works
- [x] Runs without any cloud account
- [x] `--version` works
- [x] `doctor` works
- [x] YAML policies load
- [x] Invalid policies produce useful errors
- [x] Policy version recorded in output
- [x] Rules can be named and described
- [x] Evaluation: nested fields, strings, numbers, booleans, arrays
- [x] Evaluation: equality, inequality, greater/less than, AND, OR, existence checks
- [x] Decision outputs
- [x] Diagnostics: matched rules, failed rules, explanations, stable exit codes
- [x] Interfaces: CLI, stdin/stdout, JSON, REST, Docker (Docker tested on Windows only)
- [x] Testing: policy test files, golden tests, deterministic output tests
- [x] Docs: quickstart, policy reference, CLI reference, integration example, troubleshooting
- [ ] **Success test (§51):** a new user goes from zero to a working rule in < 15 minutes without contacting you
- [ ] Tag **App Policy 1.0** and publish

## Phase 3 — Data Platform → TPT Data Validator

- [x] `tpt-schema` full: schema definition (YAML), types, required, enums, patterns, uniqueness. Note: §34 lists `tpt-schema` as a new shared repo. It does not exist on GitHub yet; it lives in this workspace.
- [~] `tpt-data-core`: record abstraction, CSV, JSON, JSON Lines, streaming. **Done:** CSV and JSON Lines stream one record at a time. **Not streamed:** a JSON array is read in full. Use JSON Lines for big files.
- [x] Validation engine using policy core + schema. Schema first, then duplicates, then the policy on valid records only (typed values, so `gt` works on CSV text).
- [x] Outputs: validated data, invalid records, error report, summary report
- [x] Row-level error messages (row, field, reason) as in §9.2
- [x] HTML report (finish `tpt-report` HTML). `tpt-data validate --html` writes `report.html`: one self-contained page, no scripts, no external links, values escaped, light and dark themes.
- [x] CLI `tpt-data validate file --schema ...`; `doctor`. Exit codes: 0 all valid, 1 I/O, 2 bad schema/policy/usage, 3 unreadable input, 10 invalid records (outputs still written).
- [x] Docker image `tpt/data` (`Dockerfile.data`). Built and tested on Docker Desktop (Windows): `doctor`, and `validate` on the example CSV (exit 10 for invalid rows, outputs written to the mounted `/out`).
- [~] Docs, examples, landing page, Gumroad listing. **Done:** `docs/DATA_VALIDATOR.md`, `docs/SCHEMA_REFERENCE.md`, examples in `examples/data/`. **Missing:** landing page, Gumroad listing.
- [~] Windows + Linux release bundle. `scripts/package.sh data` builds the `tpt-data` bundle. **Done:** Windows (Git Bash). **Done:** Linux (container). **Missing:** a clean-machine Linux install.
- [ ] Tag **Data Validator 1.0**

## Phase 4 — Document Platform → TPT Document Validator

- [x] Document schemas (JSON, XML). Same schema format as Phase 3. XML is read by fixed rules (root children are the fields, attributes as `@name`, repeated elements as lists), documented in `docs/DOCUMENT_VALIDATOR.md`.
- [x] Pipeline: parser → schema → policy → validation → report (§10.2). Schema first; the policy runs only on documents that pass.
- [x] PASS / FAIL / REVIEW outputs with detailed errors. `review` and `approval_required` give REVIEW; `rejected` and any schema or parse error give FAIL.
- [x] Templates: invoice, PO, supplier, customer, shipping records (`examples/documents/templates/`, with a policy for invoices)
- [x] CLI `tpt-document`; `doctor`; Docker image `tpt/document` (`Dockerfile.document`). **Done:** `validate` (several files, exit code = worst verdict, `--out` for per-document JSON), `doctor`, and Docker tested on Windows (Docker Desktop).
- [~] Docs, examples, landing page, Gumroad listing. **Done:** `docs/DOCUMENT_VALIDATOR.md`, examples. **Missing:** landing page, Gumroad listing.
- [~] Windows + Linux release bundle. `scripts/package.sh document` builds it. **Done:** Windows (Git Bash). **Done:** Linux (container). **Missing:** a clean-machine Linux install.
- [x] HTML report for documents. `tpt-document validate --html --out DIR` writes `report.html`: verdict counts, a document table, and each document's schema errors and policy rules. Same page rules as the data report (no scripts, escaped values, light and dark themes). Checked with a headless Edge screenshot (light theme). Dark theme not yet looked at.
- [ ] Tag **Document Validator 1.0**

## Phase 5 — Vertical → TPT Invoice Validator

- [x] Pipeline: schema → line items → subtotal → tax → total → supplier → duplicate detection → business policy (§11.2). Schema first; a failure stops the run.
- [x] PASS / REVIEW / REJECT. Any failed check in steps 1 to 7 is REJECT; the policy's `review` and `approval_required` give REVIEW.
- [~] Inputs: JSON, CSV, XML; REST; import/export files. **Done:** JSON and XML. **Missing:** CSV (invoices are one document each, so CSV needs a row-to-invoice mapping decision), REST (the `tpt-policy serve` pattern can be reused).
- [x] Duplicate detection without a database. Design: a JSON-lines ledger of `supplier tax ID|invoice number`. Accepted invoices are added at once. Documented in `docs/INVOICE_VALIDATOR.md`, including the limit that two concurrent runs can miss a duplicate.
- [x] Invoice rule/policy templates (`examples/invoices/invoice.policy.yaml`, `invoice.schema.yaml`)
- [x] CLI `tpt-invoice`; `doctor`; Docker image `tpt/invoice` (`Dockerfile.invoice`). **Done:** `validate` (with `--suppliers`, `--ledger`, `--policy`, `--out`), `doctor`, and Docker tested on Windows (Docker Desktop), including the duplicate ledger persisting across containers.
- [~] Docs, landing page, Gumroad listing, bundle. **Done:** `docs/INVOICE_VALIDATOR.md`, examples, `scripts/package.sh invoice` (Windows only). **Missing:** landing page, Gumroad listing. Linux bundle built in a container.
- [ ] Tag **Invoice Validator 1.0**

## Phase 6 — Transformation → TPT Data Transformer

- [x] Pipeline YAML engine (§13.2). A pipeline is a YAML list of single-key steps, validated before any record is read.
- [x] Transforms: rename, delete, select, trim, case, type conversion, defaults, conditional values, combine/split, map, filter, calculated fields. Field names are dotted paths. Covered by unit tests.
- [x] Formats: CSV, JSON, JSONL (in and out, with `--to`)
- [x] CLI `tpt-transform` with `run`, `check` and `doctor`
- [~] Docs, examples, landing page, Gumroad listing, bundle. **Done:** `docs/DATA_TRANSFORMER.md`, `examples/transform/` with golden output, `scripts/package.sh transform`. **Missing:** landing page, Gumroad listing; bundle not built yet.
- [ ] Tag **Data Transformer 1.0**
- [ ] (Later) joins, lookups, reference tables, large-file streaming

## Phase 7 — Approval → TPT Approval Engine

- [ ] Approval policy schema (ranges, roles, automatic decisions)
- [ ] Output: who/what must approve — no email/workflow features
- [ ] Reuse policy-core; thin product layer
- [ ] CLI `tpt-approve`; `doctor`; Docker image (optional)
- [ ] Docs, examples, landing page, Gumroad listing, bundle
- [ ] Tag **Approval Engine 1.0**

## Phase 8 — Security → TPT Secure Script Runner

- [ ] Permission manifest (filesystem read/write, network, environment, subprocess) (§12.2)
- [ ] WASM execution via tpt-wasm + tpt-capsec + tpt-runtime
- [ ] Resource limits, deterministic mode, execution logs, exit status
- [ ] Decide on optional tpt-archon / tpt-nexus hardened process boundary
- [ ] CLI `tpt-secure-run`; `doctor`
- [ ] Docs, security docs, examples (incl. denied-capability demos), landing page, Gumroad listing, bundle
- [ ] Tag **Secure Script Runner 1.0**

## Phase 9 — Compliance → TPT Compliance Evidence Processor

- [ ] Inputs: JSON, CSV, text, logs, policy files, evidence metadata
- [ ] Evidence inventory, control/evidence mapping, validation report
- [ ] Hashing + manifest + signed/verified reports via tpt-crypto
- [ ] JSON + HTML reports
- [ ] Explicit "does not certify compliance" disclaimer (§15.5)
- [ ] CLI `tpt-evidence`; `doctor`
- [ ] Docs, landing page, Gumroad listing, bundle
- [ ] Tag **Compliance Evidence Processor 1.0**

## Phase 10 — AI → TPT AI Action Guard

- [ ] Action-request schema + policy (ALLOW / DENY / REQUIRE_APPROVAL)
- [ ] tpt-mcpbox integration for MCP/tool gating
- [ ] Capability security via tpt-capsec
- [ ] CLI `tpt-ai-guard`; REST; Docker image; `doctor`
- [ ] Docs, examples (e.g. refund > $5,000), landing page, Gumroad listing, bundle
- [ ] Tag **AI Action Guard 1.0**

## Portfolio extra — TPT Rules SDK (§17)

- [ ] Rust crate API (stabilise from policy-core)
- [ ] WASM interface
- [ ] Examples, schema definitions, policy compiler, integration guide, test utilities
- [ ] Defer Python / Node.js / .NET until demand is evidenced

## Cross-cutting (repeat per product)

- [ ] `doctor` command
- [ ] Stable exit codes documented
- [ ] Report includes product + policy + schema versions
- [ ] Logs avoid customer data by default
- [ ] No cloud calls; works offline
- [ ] Semantic versioning, CHANGELOG entry, checksums, release notes
- [ ] Windows x64 + Linux x64 artefacts
- [ ] Docs set from §30 complete
- [ ] Landing page + Gumroad listing (Standard / Commercial)
- [ ] GitHub repo links the commercial product (§32)

## Explicitly NOT building (§42)

SaaS control plane · hosted DBs · mandatory accounts · ERP connectors · bespoke integrations · mobile apps · complex web dashboards · billing infrastructure · multi-tenant · 24/7 monitoring · managed deployments · customer forks · compliance consulting · custom policy authoring services · web UI first
