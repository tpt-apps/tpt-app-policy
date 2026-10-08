//! Shared CLI conventions for TPT commercial products.
//!
//! Exit codes are part of the public interface. Do not renumber them.

/// Stable process exit codes.
pub mod exit {
    /// The command succeeded (validate passed, all tests passed).
    pub const SUCCESS: u8 = 0;
    /// Decision `approved`.
    pub const APPROVED: u8 = 0;
    /// A file could not be read.
    pub const IO_ERROR: u8 = 1;
    /// The policy is invalid, or the command line is malformed.
    pub const INVALID_POLICY: u8 = 2;
    /// The input is not valid JSON.
    pub const INVALID_INPUT: u8 = 3;
    /// One or more inline tests failed.
    pub const TEST_FAILED: u8 = 4;
    /// `doctor` found at least one failing check.
    pub const DOCTOR_FAILED: u8 = 5;
    /// Decision `approval_required`.
    pub const APPROVAL_REQUIRED: u8 = 10;
    /// Decision `review`.
    pub const REVIEW: u8 = 20;
    /// Decision `rejected`.
    pub const REJECTED: u8 = 30;
}
