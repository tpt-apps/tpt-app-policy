# Changelog

## Unreleased

### Licence

- Dual-licensed MIT OR Apache-2.0 (`LICENSE-MIT`, `LICENSE-APACHE`).

Not yet tagged. Scope for 0.1 is in [TODO.md](TODO.md).

### Added

- `--verbose`, `--debug` and `--json-logs` on every product binary, from the shared `tpt-commercial-cli::log` module. Logs go to stderr and never include input values. Each run logs its start and its exit code. `tpt-policy` also logs the policy, input and decision. Documented in `docs/CLI_REFERENCE.md`.
- Config discovery: a bare policy name resolves against `./tpt/policies/`. The layout check in `doctor` uses the same shared code.
- Policy `format:` (a whole number, default 1). A policy declaring a format newer than the engine's is refused with a message to upgrade. Documented in `docs/POLICY_REFERENCE.md`.
- `examples/erp-generic/`: a generic purchase-order CSV with a schema and policy, using invented column names. Not any vendor's format.
- Policy versions are validated (`MAJOR`, `MAJOR.MINOR` or `MAJOR.MINOR.PATCH`, numbers only). An unquoted decimal such as `1.10` is rejected with a fix.
- `name@version` policy label, shown by `check`/`explain` and `validate`.
- Policy errors give the line of the problem, e.g. `rules[0].when.amount.greater_than (line 6)`.
- `tpt-policy check --format html` (and `explain`) writes one self-contained HTML page for a decision.
- `tpt-policy-wasm` crate: policy evaluation as a WebAssembly module with a plain C ABI and no imports. A Node smoke test is in `crates/tpt-policy-wasm/js/smoke.mjs`.
- `examples/embed.rs` in `tpt-policy-core`, and `docs/RULES_SDK.md`, the guide for embedding in Rust or WebAssembly.
- `tpt-policy` CLI: `validate`, `check`, `explain`, `run` (stdin), `test`, `serve`, `doctor`.
- Policy format: YAML rules with `equals`, `not_equals`, `gt`/`gte`/`lt`/`lte`,
  `in`/`not_in`, `contains`, `exists`, and `all`/`any`/`not` combinators.
- Severity-based decisions: `rejected` > `approval_required` > `review` > `approved`.
  Approvers and requirements merge across matching rules.
- Inline policy tests (`tests:` block) run with `tpt-policy test`.
- HTTP service with `POST /v1/evaluate` and `GET /healthz`. Localhost by default,
  optional bearer token from `TPT_POLICY_TOKEN`, 1 MB body limit.
- Stable exit codes, documented in the README.
- Release bundle script (`scripts/package.sh`) with SHA-256 checksums.
- Dockerfile, built and tested on Windows with Docker Desktop. Linux and macOS not yet tested.
- `tpt-report` crate: JSON and terminal reporters. The binary uses it for `check`,
  `run` and `explain`.
- Golden tests for `check` output and inline `test` runs (`crates/tpt-app-policy/tests/golden.rs`).
- Docs: install, quickstart, concepts, CLI reference, policy reference,
  integration, troubleshooting, security (in `docs/`).

### Data Validator (`tpt-data`, in development)

- Schemas in YAML: types, `required`, `unique`, `enum`, `pattern`, `min`/`max`.
- Validates CSV, JSON and JSON Lines. Writes valid and invalid records, an error
  report, `summary.json`, and optionally `report.html`.
- Optional policy check on valid records.
- JSON arrays are read one record at a time, as CSV and JSON Lines are. A broken
  array stops the run with exit `3`, and the outputs written so far are partial.

### Document Validator (`tpt-document`, in development)

- Validates JSON and XML documents against a schema, then an optional policy.
  Verdicts: PASS, REVIEW, FAIL.
- XML is read by fixed rules, documented in `docs/DOCUMENT_VALIDATOR.md`.
- Templates for invoice, purchase order, supplier, customer and shipping records.

### Invoice Validator (`tpt-invoice`, in development)

- Checks line items, subtotal, tax, total, approved suppliers and duplicates
  (file-based ledger), then an optional policy. Verdicts: PASS, REVIEW, REJECT.
- Money is checked to within one cent.
- CSV input: rows that share an invoice number form one invoice, and `line.*`
  columns give its line items. One result file per invoice.
- `tpt-invoice serve`: `POST /v1/validate` checks one invoice, and `GET /healthz`.
  Optional bearer token from `TPT_INVOICE_TOKEN`, localhost by default. The ledger
  is shared by all requests.

### Data Transformer (`tpt-transform`, in development)

- Pipelines in YAML: trim, case, rename, delete, select, type conversion, defaults,
  conditional values, combine, split, map, filter and calculated fields.
- Reads and writes CSV, JSON and JSON Lines. Rejected records go to `rejected.jsonl`
  with the reason; filtered records are counted, not rejected.
- `run`, `check` and `doctor`. Steps and exit codes are documented in
  `docs/DATA_TRANSFORMER.md`.
- `lookup` step: copies values from a reference table (CSV, JSON or JSON Lines)
  into each record, matched on a key. `on_missing: skip` or `reject`. Tables are
  read before the run, and a repeated key or a broken table stops it.
- The whole input is read into memory before the output is written. Streaming
  and joins between two inputs are not in this version.

### Approval Engine (`tpt-approve`, in development)

- Approval rules in YAML: ranges (`<1000`, `1000-5000`, `>5000`), `decision: automatic`,
  and one or more approver roles per rule.
- Compiles to a `tpt-policy-core` policy, so results and explanations match `tpt-policy`.
  A request no rule covers gets `review`, never approval.
- `validate` reports amounts no rule covers, and automatic/approval overlaps.
- `check`, `validate` and `doctor`. Steps and exit codes are documented in
  `docs/APPROVAL_ENGINE.md`.

### Compliance Evidence Processor (`tpt-evidence`, in development)

- Evidence index and controls in YAML. Maps evidence to controls, with `max_age_days`
  and `min_items`. Ages are measured to an `--as-of` date given on the command line.
- Checks each file: readable, hash matches, JSON, CSV, policy and document validity,
  and UTF-8 for text and logs.
- Writes `report.json`, `report.html` and `manifest.json` (SHA-256 of each file, and a
  hash over the manifest entries). `verify` re-checks the manifest.
- Every report says the tool does not certify compliance. Signed reports need `tpt-crypto`,
  which is not in this repository yet.
- `process`, `verify` and `doctor`. Steps and exit codes are documented in
  `docs/COMPLIANCE_EVIDENCE.md`.

### AI Action Guard (`tpt-ai-guard`, in development)

- Action policies in YAML. Each rule names an action, optional conditions in `tpt-policy`
  form, and a decision: `allow`, `deny` or `require_approval`. The spec's bare-list form
  is accepted too.
- Answers ALLOW, DENY or REQUIRE_APPROVAL with a reason. The most restrictive match wins.
  An action no rule covers is denied.
- The verdict holds a fingerprint of the request, not its fields. The guard never runs
  the action.
- `check`, `validate` and `doctor`. Steps and exit codes are documented in
  `docs/AI_ACTION_GUARD.md`.
- Not yet: MCP gating (`tpt-mcpbox`), or issuing capability tokens.

### Secure Script Runner (`tpt-secure-run`, in development)

- Runs a WebAssembly script under a YAML permission manifest. A script has no file access
  unless the manifest grants it, and a script that imports anything ungranted is refused
  before it runs (exit 7).
- File access is by manifest position (handle), never by a path from the script. Reads go
  through `tpt-capsec` tokens scoped to the granted path. Output is written only when the
  script returns 0.
- Step, memory, call-depth, file-size and log limits. The engine is deterministic, and the
  JSON report has no timestamps.
- Lifecycle follows the `tpt-runtime-core` workload states, and each step is checked against
  its transition table.
- `net_connect`, `process_spawn` and `env` are refused with a message, not ignored.
- `run`, `validate` (manifest, and optionally a module against it) and `doctor`. Steps and exit
  codes are documented in `docs/SECURE_SCRIPT_RUNNER.md`.
- Not yet: network, environment or subprocess access; string arguments to the host; a hardened
  process boundary.

### Known limits

- Single-threaded HTTP server. Not intended for direct internet exposure.
- No regex (`matches`) operator yet.
- Policy versions are recorded but not enforced against the app version.
