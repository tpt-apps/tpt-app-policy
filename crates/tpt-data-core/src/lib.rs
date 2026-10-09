//! Record reading, validation and reporting for TPT Data Validator (spec §9).
//!
//! The pipeline is: read a record, check it against a schema, optionally
//! evaluate it against a policy, then write it to the valid or invalid output.
//! Records stream through one at a time (see [`input`]). Only the values
//! needed for `unique` checks are kept between records.

use std::fmt;

pub mod input;
pub mod output;
pub mod report;
pub mod validate;

pub use input::{open, sha256_file, Format, Input, Item, Row};
pub use output::{InvalidWriter, ValidWriter};
pub use validate::{Outcome, RowError, Summary, Validator};

/// Version of this crate, recorded in reports.
pub const DATA_CORE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// An error that stops a run. Says what failed, where, why, and how to fix it.
#[derive(Debug)]
pub struct DataError {
    pub what: String,
    pub location: String,
    pub why: String,
    pub fix: String,
}

impl DataError {
    pub fn new(what: &str, location: &str, why: &str, fix: &str) -> Self {
        Self {
            what: what.to_string(),
            location: location.to_string(),
            why: why.to_string(),
            fix: fix.to_string(),
        }
    }
}

impl fmt::Display for DataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "error: {} at {}\n  why: {}\n  fix: {}",
            self.what, self.location, self.why, self.fix
        )
    }
}

impl std::error::Error for DataError {}
