# TPT Commercial Portfolio — TODO

Source: `spec.txt` (section refs in parentheses). Tick boxes as you go: `- [x]`.

## Decisions locked in

- Scope: full 10-phase portfolio, built in order (§41). Don't build products in parallel (§50).
- Layout: single Cargo workspace in this folder; split into separate repos later if wanted (§34 names kept as crate names).
- Existing TPT repos: github.com/TPT-Solutions/* (tpt-primitives, tpt-capsec, tpt-wasm, tpt-runtime, tpt-crypto, tpt-mcpbox).
- Licensing: legal licence only. No keys, no activation, no phone-home (§26).
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
- [ ] REST server crate — default: axum, optional `--token` bearer auth
- [ ] Which crates to reuse from TPT-Solutions vs. write fresh (decide after audit in Phase 2)
- [ ] Product pricing/tiers: Standard + Commercial only (§27–28)

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
- [ ] Schema abstraction + validation model + diagnostics. **Not started.** The crate exists and is empty; Phase 3 builds it out.

### tpt-report (§20)
- [ ] Common report model (status, timestamp, product, version, input, summary, errors). **Not started.** The JSON output is built directly in the binary for now and should move here.
- [ ] JSON reporter
- [ ] Terminal reporter. **Partly done:** `explain` text output lives in the binary.
- [ ] HTML reporter (can wait until Phase 3)

### tpt-commercial-cli (§18, §19, §38)
- [ ] Common flags. **Partly done:** `--format` on `check` and `explain`, and `--version`. **Missing:** `--config --policy --schema --output --json --verbose --quiet`
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
- [ ] Audit fields in output (§21). **Partly done:** engine version, policy name and version. **Missing:** product version, schema version (no schema yet), rule IDs are present but no timestamp (left out on purpose so output stays deterministic), and an optional input hash.
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
- [ ] Integrate tpt-wasm (WASM sandbox). Before embedding: run `cargo deny` on its dependency tree to check third-party licences (the tpt-wasm licence itself is MIT OR Apache-2.0).
- [ ] Resource limits (time, memory, input size)
- [ ] Deterministic mode
- [ ] Expose policy evaluation as an embeddable WASM module (§8.4)
- [ ] REST: `tpt-policy serve`, `POST /v1/evaluate`
- [ ] `doctor` subcommand: runtime, config, permissions, install, version, optional deps (§29.5)
- [ ] Signed policy bundles / release checksums via tpt-crypto (optional signatures)
- [ ] Docker image (`tpt/app-policy`) (§25)
- [ ] Release bundle layout (§23): bin, examples, policies, schemas, docs, LICENSE, CHANGELOG, README
- [ ] Windows x64 build + install test on a clean machine
- [ ] Linux x64 build + install test on a clean machine
- [ ] Checksums + release notes for each version (§40)

### Docs for 1.0 (§30)
- [ ] README
- [ ] INSTALL
- [ ] QUICKSTART
- [ ] CONCEPTS
- [ ] CONFIGURATION
- [ ] CLI_REFERENCE
- [ ] POLICY_REFERENCE
- [ ] SCHEMA_REFERENCE
- [ ] INTEGRATION (with copy/paste ERP→JSON→ERP example)
- [ ] TROUBLESHOOTING
- [ ] SECURITY
- [ ] LICENCE
- [ ] CHANGELOG

### Commercial wrapper
- [ ] Commercial licence text (personal eval / internal use / redistribution / modification / embedding / source) (§26)
- [ ] Support-boundary statement (§43)
- [ ] Privacy statement: no data leaves the machine (§39)
- [ ] Landing page (§31)
- [ ] Gumroad listings: Standard + Commercial (§27)
- [ ] Policy templates/examples (purchase order, expense, etc.)

### 1.0 acceptance criteria (§47) — do not ship until all pass
- [ ] Windows install works
- [ ] Linux install works
- [ ] Runs without any cloud account
- [ ] `--version` works
- [ ] `doctor` works
- [ ] YAML policies load
- [ ] Invalid policies produce useful errors
- [ ] Policy version recorded in output
- [ ] Rules can be named and described
- [ ] Evaluation: nested fields, strings, numbers, booleans, arrays
- [ ] Evaluation: equality, inequality, greater/less than, AND, OR, existence checks
- [ ] Decision outputs
- [ ] Diagnostics: matched rules, failed rules, explanations, stable exit codes
- [ ] Interfaces: CLI, stdin/stdout, JSON, REST, Docker
- [ ] Testing: policy test files, golden tests, deterministic output tests
- [ ] Docs: quickstart, policy reference, CLI reference, integration example, troubleshooting
- [ ] **Success test (§51):** a new user goes from zero to a working rule in < 15 minutes without contacting you
- [ ] Tag **App Policy 1.0** and publish

## Phase 3 — Data Platform → TPT Data Validator

- [ ] `tpt-schema` full: schema definition (YAML), types, required, enums, patterns, uniqueness. Note: §34 lists `tpt-schema` as a new shared repo. It does not exist on GitHub yet; it is currently an empty crate in this workspace.
- [ ] `tpt-data-core`: record abstraction, CSV, JSON, JSON Lines, streaming
- [ ] Validation engine using policy core + schema
- [ ] Outputs: validated data, invalid records, error report, summary report
- [ ] Row-level error messages (row, field, reason) as in §9.2
- [ ] HTML report (finish `tpt-report` HTML)
- [ ] CLI `tpt-data validate file --schema ...`; `doctor`
- [ ] Docker image
- [ ] Docs, examples, landing page, Gumroad listing
- [ ] Windows + Linux release bundle
- [ ] Tag **Data Validator 1.0**

## Phase 4 — Document Platform → TPT Document Validator

- [ ] Document schemas (JSON, XML)
- [ ] Pipeline: parser → schema → policy → validation → report (§10.2)
- [ ] PASS / FAIL / REVIEW outputs with detailed errors
- [ ] Templates: invoice, PO, supplier, customer, shipping records
- [ ] CLI `tpt-document`; `doctor`; Docker image
- [ ] Docs, examples, landing page, Gumroad listing
- [ ] Windows + Linux release bundle
- [ ] Tag **Document Validator 1.0**

## Phase 5 — Vertical → TPT Invoice Validator

- [ ] Pipeline: schema → line items → subtotal → tax → total → supplier → duplicate detection → business policy (§11.2)
- [ ] PASS / REVIEW / REJECT
- [ ] Inputs: JSON, CSV, XML; REST; import/export files (no direct accounting integrations)
- [ ] Duplicate detection without a database (file-based ledger of seen invoices — decide design)
- [ ] Invoice rule/policy templates
- [ ] CLI `tpt-invoice`; `doctor`; Docker image
- [ ] Docs, landing page, Gumroad listing, bundle
- [ ] Tag **Invoice Validator 1.0**

## Phase 6 — Transformation → TPT Data Transformer

- [ ] Pipeline YAML engine (§13.2)
- [ ] Transforms: rename, delete, select, trim, case, type conversion, defaults, conditional values, combine/split, map, filter, calculated fields
- [ ] Formats: CSV, JSON, JSONL
- [ ] CLI `tpt-transform`; `doctor`
- [ ] Docs, examples, landing page, Gumroad listing, bundle
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
