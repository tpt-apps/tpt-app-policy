//! Document validation pipeline for TPT Document Validator (spec §10).
//!
//! parser -> schema -> policy -> verdict
//!
//! - The parser turns a JSON or XML file into a value (see [`xml`] for the XML rules).
//! - The schema check runs first. Any schema error gives FAIL, and the policy
//!   is not run, because a policy decision should only describe a document
//!   that has the right shape.
//! - The policy, if given, gives a decision. The verdict follows from it.
//!
//! Verdicts:
//!
//! | Verdict | When |
//! |---|---|
//! | PASS | Schema passes, and the policy decision is `approved` (or there is no policy) |
//! | REVIEW | Policy decision is `review` or `approval_required` |
//! | FAIL | Schema fails, the document cannot be parsed, or the policy decision is `rejected` |

use std::fmt;
use std::fs;
use std::path::Path;

use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use tpt_policy_core::{evaluate, Decision, Evaluation, Policy};
use tpt_schema::{check_record, typed_record, FieldError, Mode, Schema};

pub mod xml;

/// The document formats this crate reads. `Csv` is not read from a file here:
/// a caller that reads CSV rows builds the record itself and uses this for
/// the schema mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DocFormat {
    Json,
    Xml,
    Csv,
}

impl DocFormat {
    /// The name used in reports.
    pub fn name(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Xml => "xml",
            Self::Csv => "csv",
        }
    }

    /// JSON values keep their types. XML and CSV values are all text.
    pub fn mode(self) -> Mode {
        match self {
            Self::Json => Mode::Strict,
            Self::Xml | Self::Csv => Mode::Text,
        }
    }

    /// The format of a file, from its extension.
    pub fn from_path(path: &Path) -> Option<Self> {
        match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
            "json" => Some(Self::Json),
            "xml" => Some(Self::Xml),
            _ => None,
        }
    }
}

/// The result for one document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Verdict {
    Pass,
    Review,
    Fail,
}

impl Verdict {
    pub fn label(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Review => "REVIEW",
            Self::Fail => "FAIL",
        }
    }

    /// Process exit code for one verdict. Matches `tpt-policy`'s codes for the
    /// same decisions: 20 review, 30 rejected.
    pub fn exit_code(self) -> u8 {
        match self {
            Self::Pass => 0,
            Self::Review => 20,
            Self::Fail => 30,
        }
    }

    /// The worse of two verdicts. FAIL is worse than REVIEW, which is worse than PASS.
    pub fn worst(self, other: Self) -> Self {
        if self.rank() >= other.rank() {
            self
        } else {
            other
        }
    }

    fn rank(self) -> u8 {
        match self {
            Self::Pass => 0,
            Self::Review => 1,
            Self::Fail => 2,
        }
    }
}

/// The verdict for a policy decision.
pub fn verdict_for(decision: Decision) -> Verdict {
    match decision {
        Decision::Approved => Verdict::Pass,
        Decision::Review | Decision::ApprovalRequired => Verdict::Review,
        Decision::Rejected => Verdict::Fail,
    }
}

/// A problem that stops a document from being read. Says what, where, why and how to fix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocError {
    pub what: String,
    pub location: String,
    pub why: String,
    pub fix: String,
}

impl fmt::Display for DocError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "error: {} at {}\n  why: {}\n  fix: {}",
            self.what, self.location, self.why, self.fix
        )
    }
}

impl std::error::Error for DocError {}

/// The outcome of parsing one document.
#[derive(Debug, Clone)]
pub enum Parsed {
    /// The document parsed. Its format and value.
    Ok { format: DocFormat, value: Value },
    /// The document could not be parsed. The message is shown as a FAIL reason.
    Malformed { format: DocFormat, message: String },
}

/// Read and parse a document. A file that cannot be read at all is an error.
/// A file that is read but is not well-formed is [`Parsed::Malformed`], so the
/// run reports it as FAIL and carries on.
pub fn read_document(path: &Path) -> Result<Parsed, DocError> {
    let format = DocFormat::from_path(path).ok_or_else(|| DocError {
        what: "unknown document type".to_string(),
        location: path.display().to_string(),
        why: "documents must be .json or .xml".to_string(),
        fix: "rename the file, or convert it to JSON or XML".to_string(),
    })?;
    let text = fs::read_to_string(path).map_err(|e| DocError {
        what: "cannot read document".to_string(),
        location: path.display().to_string(),
        why: e.to_string(),
        fix: "check the path is right and the file is readable".to_string(),
    })?;
    Ok(match format {
        // `from_path` never returns CSV, so this only guards the match.
        DocFormat::Csv => Parsed::Malformed {
            format,
            message:
                "CSV is not a document format here. Read CSV rows with the product that uses them"
                    .to_string(),
        },
        DocFormat::Xml => match xml::xml_to_value(&text) {
            Ok(value) => Parsed::Ok { format, value },
            Err(e) => Parsed::Malformed {
                format,
                message: e.message,
            },
        },
        DocFormat::Json => match serde_json::from_str::<Value>(&text) {
            Ok(value) if value.is_object() => Parsed::Ok { format, value },
            Ok(_) => Parsed::Malformed {
                format,
                message: "the JSON document is not an object. Wrap its fields in { }".to_string(),
            },
            Err(e) => Parsed::Malformed {
                format,
                message: format!("the JSON is not valid: {e}"),
            },
        },
    })
}

/// SHA-256 of a file's bytes, as lowercase hex.
pub fn file_sha256(path: &Path) -> std::io::Result<String> {
    let bytes = fs::read(path)?;
    Ok(Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

/// What the pipeline found for one document.
#[derive(Debug, Clone, Serialize)]
pub struct DocResult {
    pub verdict: Verdict,
    /// Schema failures, or the parse error as one `(document)` entry.
    pub schema_errors: Vec<FieldError>,
    /// The policy result. Present only when the schema passed and a policy was given.
    pub evaluation: Option<Evaluation>,
}

/// Run the pipeline on a parsed document.
pub fn check_document(schema: &Schema, policy: Option<&Policy>, parsed: &Parsed) -> DocResult {
    let (format, value) = match parsed {
        Parsed::Malformed { message, .. } => {
            return DocResult {
                verdict: Verdict::Fail,
                schema_errors: vec![FieldError {
                    field: "(document)".to_string(),
                    reason: message.clone(),
                }],
                evaluation: None,
            };
        }
        Parsed::Ok { format, value } => (*format, value),
    };

    let schema_errors = check_record(schema, value, format.mode());
    if !schema_errors.is_empty() {
        return DocResult {
            verdict: Verdict::Fail,
            schema_errors,
            evaluation: None,
        };
    }

    let evaluation = policy.map(|p| {
        let typed = typed_record(schema, value, format.mode());
        evaluate(p, &typed)
    });
    let verdict = match &evaluation {
        None => Verdict::Pass,
        Some(e) => verdict_for(e.decision),
    };
    DocResult {
        verdict,
        schema_errors: Vec::new(),
        evaluation,
    }
}
