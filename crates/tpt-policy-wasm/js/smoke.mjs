// Smoke test for the WebAssembly build of tpt-policy-wasm.
//
// Build the module first, then run this with Node 18 or later:
//
//   cargo build -p tpt-policy-wasm --target wasm32-unknown-unknown --release
//   node crates/tpt-policy-wasm/js/smoke.mjs
//
// It checks that the module gives the same JSON as the CLI's golden output for
// the expense example, and that bad input comes back as an error result.

import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import assert from "node:assert/strict";

const root = new URL("../../../", import.meta.url);
const wasmPath = fileURLToPath(new URL("target/wasm32-unknown-unknown/release/tpt_policy_wasm.wasm", root));
const examples = new URL("examples/expense/", root);

const { instance } = await WebAssembly.instantiate(await readFile(wasmPath), {});
const { memory, tpt_alloc, tpt_dealloc, tpt_evaluate } = instance.exports;
const encoder = new TextEncoder();
const decoder = new TextDecoder();

/** Copy text into module memory. Returns [pointer, length]. */
function write(text) {
  const bytes = encoder.encode(text);
  const ptr = tpt_alloc(bytes.length);
  new Uint8Array(memory.buffer, ptr, bytes.length).set(bytes);
  return [ptr, bytes.length];
}

/** Evaluate a policy against an input, and return the result text. */
function evaluate(policy, input) {
  const [policyPtr, policyLen] = write(policy);
  const [inputPtr, inputLen] = write(input);
  // The module returns a u64: pointer in the high 32 bits, length in the low 32.
  const packed = tpt_evaluate(policyPtr, policyLen, inputPtr, inputLen);
  const resultPtr = Number(packed >> 32n);
  const resultLen = Number(packed & 0xffffffffn);
  const text = decoder.decode(new Uint8Array(memory.buffer, resultPtr, resultLen));
  tpt_dealloc(resultPtr, resultLen);
  tpt_dealloc(policyPtr, policyLen);
  tpt_dealloc(inputPtr, inputLen);
  return text;
}

const policy = await readFile(new URL("expense.yaml", examples), "utf8");
const input = await readFile(new URL("expense.json", examples), "utf8");
const expected = await readFile(new URL("expense.expected.json", examples), "utf8");

// The CLI prints a trailing newline. The module does not.
assert.equal(evaluate(policy, input) + "\n", expected, "same output as the CLI");

const badPolicy = JSON.parse(evaluate("policy: x\nrules: []\n", "{}"));
assert.equal(badPolicy.error.what, "no rules");

const badInput = JSON.parse(evaluate(policy, "{nope"));
assert.equal(badInput.error.what, "invalid input JSON");

// Many calls, to check memory is returned and reused.
for (let i = 0; i < 2000; i++) {
  evaluate(policy, input);
}

console.log("tpt-policy-wasm smoke test: ok");
