//! Policy evaluation as a WebAssembly module (spec §8.4), for embedding.
//!
//! The module has no imports and keeps no state. A host gives it a policy and
//! an input as UTF-8 text, and gets back the result as UTF-8 JSON. The JSON is
//! the same as `tpt-policy check` prints, so a host and the CLI agree.
//!
//! # Interface
//!
//! All text is passed as a pointer and a length into the module's memory.
//!
//! - `tpt_alloc(len) -> ptr`: reserve `len` bytes. The host writes text there.
//! - `tpt_dealloc(ptr, len)`: free bytes from `tpt_alloc`, or a result.
//! - `tpt_evaluate(policy_ptr, policy_len, input_ptr, input_len) -> u64`:
//!   evaluate a policy (YAML) against an input (JSON). Returns the result as a
//!   packed `u64`: the pointer in the high 32 bits, the length in the low 32
//!   bits. The result is JSON, either the evaluation or `{"error": ...}`.
//!   The host frees the result with `tpt_dealloc`. It also frees its own
//!   policy and input buffers, since the module does not keep them.
//!
//! Errors are data, not traps. A bad policy, a bad input or bad UTF-8 all give
//! an `{"error": {"what", "location", "why", "fix"}}` result.
//!
//! The JSON is returned without a trailing newline. The CLI adds one.

#![deny(unsafe_op_in_unsafe_fn)]

use serde_json::{json, Value};
use tpt_policy_core::{evaluate, parse_policy, PolicyError};

/// Evaluate `input_json` against `policy_yaml`. Always returns JSON text.
pub fn evaluate_text(policy_yaml: &str, input_json: &str) -> String {
    let policy = match parse_policy(policy_yaml) {
        Ok(policy) => policy,
        Err(e) => return error_json(&e),
    };
    let input: Value = match serde_json::from_str(input_json) {
        Ok(input) => input,
        Err(e) => {
            return json!({"error": {
                "what": "invalid input JSON",
                "location": "input",
                "why": e.to_string(),
                "fix": "send the input as one JSON value",
            }})
            .to_string();
        }
    };
    tpt_report::json(&evaluate(&policy, &input))
}

fn error_json(e: &PolicyError) -> String {
    json!({"error": {
        "what": e.what,
        "location": e.location,
        "why": e.why,
        "fix": e.fix,
        "line": e.line,
    }})
    .to_string()
}

/// Pack a result into one `u64`: pointer in the high 32 bits, length in the low.
fn pack(text: String) -> u64 {
    let bytes = text.into_bytes().into_boxed_slice();
    let len = bytes.len();
    let ptr = Box::into_raw(bytes) as *mut u8 as usize;
    // wasm32 addresses are 32 bits, so both halves fit.
    debug_assert!(ptr <= u32::MAX as usize && len <= u32::MAX as usize);
    ((ptr as u64) << 32) | (len as u64)
}

// --- C ABI -------------------------------------------------------------------

/// Reserve `len` bytes for the host to write into. Free with `tpt_dealloc`.
#[no_mangle]
pub extern "C" fn tpt_alloc(len: usize) -> *mut u8 {
    let bytes = vec![0u8; len].into_boxed_slice();
    Box::into_raw(bytes) as *mut u8
}

/// Free `len` bytes at `ptr`, which must come from `tpt_alloc` or `tpt_evaluate`.
///
/// # Safety
///
/// `ptr` and `len` must be exactly the pair returned by one earlier call, and
/// must not have been freed already.
#[no_mangle]
pub unsafe extern "C" fn tpt_dealloc(ptr: *mut u8, len: usize) {
    let slice = std::ptr::slice_from_raw_parts_mut(ptr, len);
    // SAFETY: the caller passes a pointer and length from `tpt_alloc` or
    // `tpt_evaluate`, which came from `Box<[u8]>` of this length.
    drop(unsafe { Box::from_raw(slice) });
}

/// Evaluate a policy against an input. See the module docs for the contract.
///
/// # Safety
///
/// Each pointer must be valid for reads of its length, and hold text the host
/// wrote. The host keeps the buffers alive for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn tpt_evaluate(
    policy_ptr: *const u8,
    policy_len: usize,
    input_ptr: *const u8,
    input_len: usize,
) -> u64 {
    // SAFETY: the host passes valid pointers and lengths (see the function docs).
    let policy = unsafe { std::slice::from_raw_parts(policy_ptr, policy_len) };
    // SAFETY: as above, for the input buffer.
    let input = unsafe { std::slice::from_raw_parts(input_ptr, input_len) };
    let reply = match (std::str::from_utf8(policy), std::str::from_utf8(input)) {
        (Ok(policy), Ok(input)) => evaluate_text(policy, input),
        _ => json!({"error": {
            "what": "invalid text",
            "location": "policy or input",
            "why": "the text is not valid UTF-8",
            "fix": "send the policy as YAML text and the input as JSON text, both in UTF-8",
        }})
        .to_string(),
    };
    pack(reply)
}

#[cfg(test)]
mod tests {
    use super::*;

    const POLICY: &str = "\
policy: expenses
rules:
  - id: big
    when:
      amount:
        gt: 1000
    then:
      decision: approval_required
      approver: manager
  - id: small
    when:
      amount:
        lte: 1000
    then:
      decision: approved
";

    #[test]
    fn evaluates_like_the_cli_json_reporter() {
        let out: Value =
            serde_json::from_str(&evaluate_text(POLICY, r#"{"amount": 1500}"#)).unwrap();
        assert_eq!(out["decision"], "approval_required");
        assert_eq!(out["approvers"][0], "manager");
    }

    #[test]
    fn a_bad_policy_is_an_error_result() {
        let out: Value =
            serde_json::from_str(&evaluate_text("policy: x\nrules: []\n", "{}")).unwrap();
        assert_eq!(out["error"]["what"], "no rules");
    }

    #[test]
    fn a_bad_input_is_an_error_result() {
        let out: Value = serde_json::from_str(&evaluate_text(POLICY, "{nope")).unwrap();
        assert_eq!(out["error"]["what"], "invalid input JSON");
    }

    #[test]
    fn output_is_deterministic() {
        assert_eq!(
            evaluate_text(POLICY, r#"{"amount": 5}"#),
            evaluate_text(POLICY, r#"{"amount": 5}"#)
        );
    }
}
