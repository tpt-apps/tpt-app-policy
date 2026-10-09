# Rules SDK: embedding the policy engine

The policy engine can be used from your own code in two ways. Use the Rust
crate if your program is in Rust. Use the WebAssembly module from any other
language that can load WebAssembly, such as Node.js, Python with a Wasm runtime,
or .NET.

Both give the same results as the `tpt-policy` command. The rules are the same
YAML, and the output is the same JSON.

## Rust crate: `tpt-policy-core`

Add the crate as a path or git dependency. Its public items are:

| Item | What it is for |
|---|---|
| `parse_policy(yaml) -> Result<Policy, PolicyError>` | Parse and check a policy. Parse once, then evaluate many times. |
| `evaluate(&policy, &input) -> Evaluation` | Evaluate one JSON input. Pure: no clock, no randomness, no I/O. |
| `run_tests(&policy) -> Vec<TestOutcome>` | Run the policy's own `tests:` block. For your test suite. |
| `Evaluation` | The decision, approvers, requirements, warnings, matched rules, explanations and versions. |
| `Decision` | `Approved`, `Review`, `ApprovalRequired` or `Rejected`. Ordered by severity. |
| `PolicyError` | `what`, `location`, `why`, `fix`, and `line` when the line is known. |
| `ENGINE_VERSION` | The engine version, recorded in every evaluation. |

A complete program is in
[`crates/tpt-policy-core/examples/embed.rs`](../crates/tpt-policy-core/examples/embed.rs).
Run it with `cargo run -p tpt-policy-core --example embed`.

```rust
use serde_json::json;
use tpt_policy_core::{evaluate, parse_policy, Decision};

let policy = parse_policy(&std::fs::read_to_string("expense.yaml")?)?;
let evaluation = evaluate(&policy, &json!({"amount": 2500}));
match evaluation.decision {
    Decision::Approved => { /* proceed */ }
    Decision::Review | Decision::ApprovalRequired => { /* send to approvers */ }
    Decision::Rejected => { /* stop */ }
}
```

### Stability

- The `Decision` names, the JSON output of `evaluate` (the `tpt-policy check`
  format), and the policy YAML format are the interface to depend on. Changes
  to them are recorded in `CHANGELOG.md`.
- Adding a field to a public type is a change you may need to handle, so match
  on `Decision` with a `_` arm if you want to survive new decisions.
- The engine is deterministic: the same policy and input give byte-identical
  output. Your tests can compare output as text.

## WebAssembly module: `tpt-policy-wasm`

The module has no imports and no state. Build it with:

```
cargo build -p tpt-policy-wasm --target wasm32-unknown-unknown --release
```

The output is `target/wasm32-unknown-unknown/release/tpt_policy_wasm.wasm`.
It is about 470 KB before any further size optimisation.

Its exports are a plain C ABI. Text is passed as a pointer and a length into the
module's memory:

| Export | Signature | What it does |
|---|---|---|
| `tpt_alloc` | `(len) -> ptr` | Reserve `len` bytes. Write the policy or input text there. |
| `tpt_evaluate` | `(policy_ptr, policy_len, input_ptr, input_len) -> u64` | Evaluate. Returns the result as a packed `u64`: pointer in the high 32 bits, length in the low 32 bits. |
| `tpt_dealloc` | `(ptr, len)` | Free a buffer from `tpt_alloc`, or a result from `tpt_evaluate`. |

The policy is YAML text and the input is JSON text, both in UTF-8. The result is
UTF-8 JSON: the evaluation, or `{"error": {"what", "location", "why", "fix"}}`.
A bad policy, a bad input, or bad UTF-8 is an error result, not a trap. The
host frees the result and its own buffers with `tpt_dealloc`.

The module is stateless, so each call parses the policy again. For a policy
evaluated many times, keep a copy of the result rather than the policy text.

### Example: Node.js

The smoke test in
[`crates/tpt-policy-wasm/js/smoke.mjs`](../crates/tpt-policy-wasm/js/smoke.mjs)
is a complete host. Its core is:

```js
const { instance } = await WebAssembly.instantiate(wasmBytes, {});
const { memory, tpt_alloc, tpt_dealloc, tpt_evaluate } = instance.exports;

function write(text) {
  const bytes = new TextEncoder().encode(text);
  const ptr = tpt_alloc(bytes.length);
  new Uint8Array(memory.buffer, ptr, bytes.length).set(bytes);
  return [ptr, bytes.length];
}

const [pp, pl] = write(policyYaml);
const [ip, il] = write(inputJson);
const packed = tpt_evaluate(pp, pl, ip, il);           // a BigInt
const rp = Number(packed >> 32n), rl = Number(packed & 0xffffffffn);
const result = new TextDecoder().decode(new Uint8Array(memory.buffer, rp, rl));
tpt_dealloc(rp, rl);
tpt_dealloc(pp, pl);
tpt_dealloc(ip, il);
```

Read `memory.buffer` after each call that may allocate. A wasm memory that grows
is replaced, and an old view of it is detached.

Run the smoke test with Node 18 or later:

```
node crates/tpt-policy-wasm/js/smoke.mjs
```

It checks the expense example against the CLI's golden output, and runs 2,000
evaluations to check that memory is returned.

## Test utilities

`run_tests` runs the `tests:` block in a policy file, the same as
`tpt-policy test`. Put policy tests in your own test suite:

```rust
#[test]
fn expense_policy_passes_its_tests() {
    let policy = parse_policy(include_str!("../policies/expense.yaml")).unwrap();
    for outcome in run_tests(&policy) {
        assert!(outcome.passed, "{}: {}", outcome.name, outcome.message);
    }
}
```

## Not in this version

- SDKs for Python, Node.js and .NET are not published. The WebAssembly module
  is the way to reach them, and a native binding is deferred until there is
  evidence that people need one.
- A separate policy compiler. `parse_policy` already checks and compiles a policy
  to the evaluated form, and reports errors with a key path and line.
- A signed module. Release checksums are in `SHA256SUMS` from the packaging
  script. Signatures wait on the `tpt-crypto` decision.
