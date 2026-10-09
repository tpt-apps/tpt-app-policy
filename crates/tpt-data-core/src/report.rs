//! Human-readable error report, one block per invalid record:
//!
//! ```text
//! Row 182:
//!   email: 'bad' does not match the pattern ^[^@\s]+@[^@\s]+\.[^@\s]+$
//! ```
//!
//! The same text goes to `errors.txt` and to the console.

use std::fmt::Write as _;

use crate::validate::Outcome;

/// The error block for one invalid record. Empty for a valid record.
pub fn row_block(outcome: &Outcome) -> String {
    if outcome.is_valid() {
        return String::new();
    }
    let mut text = format!("Row {}:\n", outcome.number);
    for error in &outcome.errors {
        // Writing to a String cannot fail.
        let _ = writeln!(text, "  {}: {}", error.field, error.reason);
    }
    text
}
