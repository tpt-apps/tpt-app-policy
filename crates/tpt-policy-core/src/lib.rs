//! Deterministic policy evaluation engine.
//!
//! A policy is a list of rules. Each rule has a `when` condition over JSON
//! input and a `then` outcome. Evaluation is pure: the same input and the
//! same policy always produce the same [`Evaluation`].

mod error;
mod eval;
mod locate;
mod model;
mod parse;
mod testing;

pub use error::PolicyError;
pub use eval::{evaluate, Evaluation, Explanation, FailedRule, PolicyRef};
pub use model::{Check, Condition, Decision, Expectation, Op, Outcome, Policy, Rule, TestCase};
pub use parse::parse_policy;
pub use testing::{run_tests, TestOutcome};

/// Engine version, recorded in every evaluation for auditability.
pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");
