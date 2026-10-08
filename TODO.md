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
- [ ] Combinator syntax for AND/OR/NOT — default: `all:` / `any:` / `not:` blocks; sibling fields are implicit AND
- [ ] Operator set — default: `equals, not_equals, gt, gte, lt, lte, in, not_in, contains, exists, matches`
- [ ] Dotted paths (`expense.amount`) and nested YAML both supported — default: yes
- [ ] Missing field behaviour — default: condition is false (not an error) unless `exists` used
- [ ] Decision when no rule matches — default: policy-level `default_decision`, required field
- [ ] Final exit-code table (e.g. 0 approved, 10 approval_required, 20 review, 30 rejected, 2 policy error, 3 input error)
- [ ] REST server crate — default: axum, optional `--token` bearer auth
- [ ] Which crates to reuse from TPT-Solutions vs. write fresh (decide after audit in Phase 2)
- [ ] Product pricing/tiers: Standard + Commercial only (§27–28)

## Phase 0 — Workspace & shared groundwork

- [ ] Init git repo, `.gitignore`, `rust-toolchain.toml`
- [ ] Create Cargo workspace root
- [ ] Create crates: `tpt-policy-core`, `tpt-schema`, `tpt-report`, `tpt-commercial-cli`, `tpt-app-policy`
- [ ] CI (build + test + clippy + fmt) for Windows and Linux
- [ ] Clone/audit TPT-Solutions repos: tpt-primitives, tpt-capsec, tpt-wasm, tpt-runtime, tpt-crypto, tpt-mcpbox
- [ ] Define the narrow stable dependency layer (§33); record API gaps
- [ ] Pick workspace dependencies (serde, serde_yaml, serde_json, clap, thiserror, ...)
- [ ] Write `docs/ARCHITECTURE.md` (JSON boundary, layering, what NOT to build §42)

## Phase 1 — Policy Foundation → TPT App Policy 0.1

### tpt-policy-core (§7, §34, §35)
- [ ] Rule AST
- [ ] YAML parser → AST with line/column in errors
- [ ] Policy metadata: `policy`, `version`, rule `id`, `description`
- [ ] Field-path resolution (nested objects, arrays)
- [ ] Operators: equality, inequality, gt/gte/lt/lte, string compare, list membership, existence
- [ ] Types: strings, numbers, booleans, arrays, nested objects
- [ ] AND / OR (and NOT) conditions
- [ ] Decision model + severity precedence merge
- [ ] Merged outputs: approvers, requirements, warnings
- [ ] Explanation model: matched rules, failed rules/checks, why
- [ ] Policy versioning (independent of app version, `name@x.y.z`) (§37)
- [ ] Determinism guarantees (stable ordering, no clock/random in evaluation) (§7.4)
- [ ] Policy validation with actionable errors: what/where/why/how to fix (§29.4)
- [ ] Inline policy `tests:` block (name / input / expect) (§36)
- [ ] Unit tests + golden tests + deterministic-output tests

### tpt-schema (minimal at this stage) (§34)
- [ ] Schema abstraction + validation model + diagnostics (stub enough for policy input checks)

### tpt-report (§20)
- [ ] Common report model (status, timestamp, product, version, input, summary, errors)
- [ ] JSON reporter
- [ ] Terminal reporter
- [ ] HTML reporter (can wait until Phase 3)

### tpt-commercial-cli (§18, §19, §38)
- [ ] Common flags: `--config --policy --schema --input --output --format --json --verbose --quiet --version`
- [ ] `--debug`, `--json-logs`; logs never dump customer records by default
- [ ] Config discovery (`./tpt/config|policies|schemas|templates|reports`), no hidden DB
- [ ] Diagnostics formatter
- [ ] Stable, documented exit codes

### tpt-app-policy binary `tpt-policy` (§8, §47–48)
- [ ] `validate policy.yaml`
- [ ] `check policy.yaml input.json`
- [ ] `explain policy.yaml input.json`
- [ ] `test policy.yaml` (PASS/FAIL output + summary)
- [ ] `run policy.yaml` via stdin/stdout
- [ ] `--version`
- [ ] Audit fields in output: product version, policy version, schema version, rule IDs, timestamp, optional input hash (§21)
- [ ] §48 expense demo passes exactly as written
- [ ] README + QUICKSTART draft
- [ ] Tag **App Policy 0.1**

## Phase 2 — Production Runtime → TPT App Policy 1.0

- [ ] Integrate tpt-primitives (types/IDs/deterministic representations)
- [ ] Integrate tpt-capsec (capability model)
- [ ] Integrate tpt-runtime (lifecycle, resource limits)
- [ ] Integrate tpt-wasm (WASM sandbox)
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

- [ ] `tpt-schema` full: schema definition (YAML), types, required, enums, patterns, uniqueness
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
