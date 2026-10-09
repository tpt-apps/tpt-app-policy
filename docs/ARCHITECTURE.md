# Architecture

Source of truth for scope: [spec.txt](../spec.txt). Work tracking: [TODO.md](../TODO.md).

## Layering

```
Product (tpt-app-policy → tpt-policy)
  └─ tpt-commercial-cli   flags, config discovery, diagnostics, exit codes
  └─ tpt-report           JSON / terminal / HTML report model
  └─ tpt-schema           schema abstraction and validation model
  └─ tpt-policy-core      rule AST, parser, evaluator, decision model
        └─ TPT foundation (github.com/TPT-Solutions)
             tpt-primitives, tpt-capsec, tpt-wasm, tpt-runtime, tpt-crypto, tpt-mcpbox
```

Customer systems sit outside the platform. The product accepts JSON and
returns a JSON decision. It never talks to an ERP, database or cloud service.

## Foundation audit (Phase 0)

Checked against the default branches of each repo on 2026-10-09.

| Repo | Licence | Relevant finding |
|---|---|---|
| tpt-primitives | Apache-2.0 / MIT | Canonical identity, capability and evidence types. Candidate for shared IDs and deterministic representations. |
| tpt-capsec | Apache-2.0 / MIT | **Compile-time** type-state capability tokens, not a runtime permission checker. Fits Secure Script Runner and AI Guard only if the permission model is compiled in. Does not replace a runtime manifest. |
| tpt-wasm | Apache-2.0 / MIT (dual, same as the others). GitHub shows "Other", probably from its licence detection, not the licence itself. | Independent WASM implementation, not a fork of any runtime. Dependency licence record is still an unpopulated placeholder, so third-party crate licences must be checked (`cargo deny`) before Phase 2 embedding. |
| tpt-runtime | Apache-2.0 / MIT | Large workspace (21 crates). Contains `tpt-runtime-policy`, which overlaps with `tpt-policy-core`. Decide reuse vs. independent before Phase 2. |
| tpt-crypto | Apache-2.0 / MIT | Pure Rust, no_std-first. Suitable for signed bundles and evidence hashes. |
| tpt-mcpbox | Apache-2.0 / MIT | Has `tpt-mcpbox-policy`, also overlapping. Relevant to Phase 10. |

### Open questions raised by the audit

1. Do the third-party crates that `tpt-wasm` depends on permit selling a binary that embeds them? (The `tpt-wasm` licence itself is MIT OR Apache-2.0.)
2. Should `tpt-policy-core` be independent of `tpt-runtime-policy` / `tpt-mcpbox-policy`? Current assumption: **yes**, to keep the commercial core small and stable (spec §33, "narrow stable dependency layer").
3. Is `tpt-capsec`'s compile-time model acceptable for plugin sandboxing, given plugins are loaded at runtime?

## Foundation integration (Phase 2)

Pinned once in the workspace `Cargo.toml`. Each repo has no tags, so each is pinned to a commit.

| Crate | Used by | What is used | Not used, and why |
|---|---|---|---|
| tpt-capsec-core | tpt-ai-guard, tpt-secure-run | Scope checks; `RootCapability` and `FsReadToken` | `tpt-capsec` wrapper crate. Its checks are a convention in our code, not an OS barrier. |
| tpt-primitives | tpt-secure-run | `ArtifactId` for module identity | Policy hashing stays on `sha2`. The output is identical, so moving it would only churn code. |
| tpt-runtime-core | tpt-secure-run | `WorkloadState` lifecycle table | `tpt-runtime-policy`: it overlaps `tpt-policy-core`, and we keep the commercial core independent. |
| tpt-wasm-runtime, -types, -format, -decode | tpt-secure-run | Deterministic engine, limits, module decode and imports | Its full compiler crates and the `tpt-wasm` host, which the runner does not need. |

The dependency tree has no HTTP or TLS client (checked with `cargo tree`).

## Non-goals

See spec §42. In short: no SaaS control plane, no hosted databases, no
mandatory accounts, no ERP connectors, no web UI before the CLI is excellent,
and no licence keys or phone-home.
