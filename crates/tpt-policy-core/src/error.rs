//! Diagnostics for invalid policy files.
//!
//! Every error says what failed, where, why, and how to fix it (spec §29.4).

use std::fmt;

/// A policy validation error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyError {
    /// Short name of the problem, e.g. `unknown operator`.
    pub what: String,
    /// Where it happened, e.g. `rules[0].when.amount` or `line 4, column 3`.
    pub location: String,
    /// Why this is a problem.
    pub why: String,
    /// How to fix it.
    pub fix: String,
}

impl PolicyError {
    pub(crate) fn new(
        what: impl Into<String>,
        location: impl Into<String>,
        why: impl Into<String>,
        fix: impl Into<String>,
    ) -> Self {
        Self {
            what: what.into(),
            location: location.into(),
            why: why.into(),
            fix: fix.into(),
        }
    }
}

impl fmt::Display for PolicyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "error: {} at {}\n  why: {}\n  fix: {}",
            self.what, self.location, self.why, self.fix
        )
    }
}

impl std::error::Error for PolicyError {}
