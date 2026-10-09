# Changelog

## Unreleased

### Licence

- Dual-licensed MIT OR Apache-2.0 (`LICENSE-MIT`, `LICENSE-APACHE`).

Not yet tagged. Scope for 0.1 is in [TODO.md](TODO.md).

### Added

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
- Dockerfile, not yet built or tested.
- `tpt-report` crate: JSON and terminal reporters. The binary uses it for `check`,
  `run` and `explain`.
- Golden tests for `check` output and inline `test` runs (`crates/tpt-app-policy/tests/golden.rs`).
- Docs: install, quickstart, concepts, CLI reference, policy reference,
  integration, troubleshooting, security (in `docs/`).

### Known limits

- Single-threaded HTTP server. Not intended for direct internet exposure.
- No regex (`matches`) operator yet.
- No schema support yet (`tpt-schema` is an empty crate).
- Policy versions are recorded but not enforced against the app version.
