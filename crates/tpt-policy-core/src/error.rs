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
    /// The line in the policy file, 1-based, where the problem is. `None` when
    /// the error is about the whole file, or the line cannot be found.
    pub line: Option<usize>,
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
            line: None,
        }
    }
}

impl fmt::Display for PolicyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The key path, then the line when it is known.
        let place = match self.line {
            Some(n) => format!("{} (line {n})", self.location),
            None => self.location.clone(),
        };
        write!(
            f,
            "error: {} at {}\n  why: {}\n  fix: {}",
            self.what, place, self.why, self.fix
        )
    }
}

impl std::error::Error for PolicyError {}
