//! TPT Invoice Validator (spec §11): checks invoices before they enter accounting.
//!
//! Pipeline (§11.2), in order:
//!
//! 1. schema: the header fields, checked by `tpt-schema`
//! 2. line items: each line's amount equals quantity x unit price
//! 3. subtotal: equals the sum of the line amounts
//! 4. tax: equals subtotal x tax rate, when the invoice states a rate
//! 5. total: equals subtotal + tax
//! 6. supplier: the supplier's tax ID is on the approved list, when a list is given
//! 7. duplicates: this supplier and invoice number are not already in the ledger
//! 8. business policy: an optional `tpt-policy` policy, as in the other products
//!
//! Any failure in steps 1 to 7 is REJECT. A policy result then sets the verdict:
//! `rejected` gives REJECT, `review` and `approval_required` give REVIEW.
//!
//! Money is checked to within one cent. Amounts are read as numbers, so text
//! from XML is accepted when it holds a number.

use std::collections::HashSet;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{json, Value};
use tpt_document::Parsed;
use tpt_policy_core::{evaluate, Decision, Evaluation, Policy};
use tpt_schema::{check_record, lookup, typed_record, FieldError, Mode, Schema};

/// Largest difference allowed between two money amounts that should agree.
pub const CENT: f64 = 0.01;

/// The invoice verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Verdict {
    Pass,
    Review,
    Reject,
}

impl Verdict {
    pub fn label(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Review => "REVIEW",
            Self::Reject => "REJECT",
        }
    }

    /// Exit code. Matches `tpt-policy` and `tpt-document` for review (20) and
    /// rejected (30).
    pub fn exit_code(self) -> u8 {
        match self {
            Self::Pass => 0,
            Self::Review => 20,
            Self::Reject => 30,
        }
    }

    /// The worse of two verdicts.
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
            Self::Reject => 2,
        }
    }
}

/// One step of the pipeline, with what it found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Check {
    pub name: &'static str,
    pub passed: bool,
    /// Explains a failure, or notes a step that was skipped.
    pub message: String,
}

/// The result for one invoice.
#[derive(Debug, Clone, Serialize)]
pub struct InvoiceResult {
    pub verdict: Verdict,
    /// Header fields that failed the schema, or the parse error.
    pub schema_errors: Vec<FieldError>,
    /// Steps 2 to 7. Empty when the schema failed.
    pub checks: Vec<Check>,
    /// Step 8. Present when the invoice passed steps 1 to 7 and a policy was given.
    pub evaluation: Option<Evaluation>,
    /// The duplicate key, when the ledger already had this invoice.
    pub duplicate_key: Option<String>,
}

/// Approved suppliers, by tax ID.
#[derive(Debug, Clone, Default)]
pub struct Suppliers {
    tax_ids: HashSet<String>,
}

impl Suppliers {
    /// Parse a JSON array of tax IDs, e.g. `["123-456-789"]`.
    pub fn parse(text: &str) -> Result<Self, String> {
        let list: Vec<String> = serde_json::from_str(text)
            .map_err(|e| format!("the supplier list must be a JSON array of tax IDs: {e}"))?;
        Ok(Self {
            tax_ids: list.into_iter().map(|s| s.trim().to_string()).collect(),
        })
    }

    pub fn contains(&self, tax_id: &str) -> bool {
        self.tax_ids.contains(tax_id.trim())
    }
}

/// The record of invoices already processed. A file of JSON lines, one
/// `{"key": ...}` per invoice, so duplicates are caught across runs.
#[derive(Debug)]
pub struct Ledger {
    path: PathBuf,
    keys: HashSet<String>,
}

impl Ledger {
    /// Load the ledger. A missing file is an empty ledger, created on first append.
    pub fn load(path: &Path) -> io::Result<Self> {
        let mut keys = HashSet::new();
        if path.exists() {
            for line in BufReader::new(fs::File::open(path)?).lines() {
                let line = line?;
                if line.trim().is_empty() {
                    continue;
                }
                let value: Value = serde_json::from_str(&line).map_err(|e| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("ledger line is not JSON: {e}"),
                    )
                })?;
                if let Some(key) = value.get("key").and_then(Value::as_str) {
                    keys.insert(key.to_string());
                }
            }
        }
        Ok(Self {
            path: path.to_path_buf(),
            keys,
        })
    }

    pub fn contains(&self, key: &str) -> bool {
        self.keys.contains(key)
    }

    /// Record a key so later runs see it. Keys already present are not written again.
    pub fn record(&mut self, key: &str) -> io::Result<()> {
        if !self.keys.insert(key.to_string()) {
            return Ok(());
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        writeln!(file, "{}", json!({ "key": key }))
    }
}

/// The duplicate key for an invoice: supplier tax ID and invoice number.
pub fn duplicate_key(record: &Value) -> Option<String> {
    let tax_id = text_of(lookup(record, "supplier.tax_id")?)?;
    let number = text_of(lookup(record, "invoice_number")?)?;
    Some(format!("{tax_id}|{number}"))
}

/// Run the pipeline on one parsed invoice.
pub fn check_invoice(
    schema: &Schema,
    policy: Option<&Policy>,
    suppliers: Option<&Suppliers>,
    ledger: Option<&Ledger>,
    parsed: &Parsed,
) -> InvoiceResult {
    let (format, value) = match parsed {
        Parsed::Malformed { message, .. } => {
            return reject_without_checks(vec![FieldError {
                field: "(document)".to_string(),
                reason: message.clone(),
            }]);
        }
        Parsed::Ok { format, value } => (*format, value),
    };
    let mode: Mode = format.mode();

    let schema_errors = check_record(schema, value, mode);
    if !schema_errors.is_empty() {
        return reject_without_checks(schema_errors);
    }

    let mut checks = vec![
        check_lines(value),
        check_subtotal(value),
        check_tax(value),
        check_total(value),
        check_supplier(value, suppliers),
    ];

    let key = duplicate_key(value);
    let duplicate = match (&key, ledger) {
        (Some(k), Some(l)) if l.contains(k) => Some(k.clone()),
        _ => None,
    };
    checks.push(Check {
        name: "duplicate",
        passed: duplicate.is_none(),
        message: match (&duplicate, ledger) {
            (Some(_), _) => {
                let number = lookup(value, "invoice_number")
                    .and_then(text_of)
                    .unwrap_or_default();
                let tax_id = lookup(value, "supplier.tax_id")
                    .and_then(text_of)
                    .unwrap_or_default();
                format!("invoice {number} from supplier {tax_id} was already processed")
            }
            (None, None) => "no ledger given, duplicates are not checked".to_string(),
            (None, Some(_)) => "not in the ledger".to_string(),
        },
    });

    let passed_checks = checks.iter().all(|c| c.passed);
    let evaluation = match (passed_checks, policy) {
        (true, Some(p)) => Some(evaluate(p, &typed_record(schema, value, mode))),
        _ => None,
    };
    let verdict = if !passed_checks {
        Verdict::Reject
    } else {
        match &evaluation {
            None => Verdict::Pass,
            Some(e) => verdict_for(e.decision),
        }
    };

    InvoiceResult {
        verdict,
        schema_errors: Vec::new(),
        checks,
        evaluation,
        duplicate_key: duplicate,
    }
}

fn reject_without_checks(schema_errors: Vec<FieldError>) -> InvoiceResult {
    InvoiceResult {
        verdict: Verdict::Reject,
        schema_errors,
        checks: Vec::new(),
        evaluation: None,
        duplicate_key: None,
    }
}

/// The verdict for a policy decision.
pub fn verdict_for(decision: Decision) -> Verdict {
    match decision {
        Decision::Approved => Verdict::Pass,
        Decision::Review | Decision::ApprovalRequired => Verdict::Review,
        Decision::Rejected => Verdict::Reject,
    }
}

/// The line items. JSON gives `lines` as a list. XML gives `<lines><line>...</line></lines>`,
/// which is a single object when there is one line, and a list when there are several.
fn line_items(value: &Value) -> Option<Vec<&Value>> {
    match lookup(value, "lines")? {
        Value::Array(items) => Some(items.iter().collect()),
        Value::Object(map) => match map.get("line")? {
            Value::Array(items) => Some(items.iter().collect()),
            single @ Value::Object(_) => Some(vec![single]),
            _ => None,
        },
        _ => None,
    }
}

fn check_lines(value: &Value) -> Check {
    let name = "line items";
    let Some(lines) = line_items(value) else {
        return fail(name, "lines must be a list of line items");
    };
    if lines.is_empty() {
        return fail(name, "the invoice has no line items");
    }
    let mut problems = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let number = i + 1;
        let (Some(q), Some(p), Some(a)) = (
            number_at(line, "quantity"),
            number_at(line, "unit_price"),
            number_at(line, "amount"),
        ) else {
            problems.push(format!(
                "line {number}: needs quantity, unit_price and amount as numbers"
            ));
            continue;
        };
        if (q * p - a).abs() > CENT {
            problems.push(format!(
                "line {number}: amount {} is not quantity {} x unit price {} = {:.2}",
                money(a),
                money(q),
                money(p),
                q * p
            ));
        }
    }
    if problems.is_empty() {
        pass(
            name,
            &format!(
                "{}: each amount = quantity x unit price",
                plural(lines.len(), "line", "lines")
            ),
        )
    } else {
        fail(name, &problems.join("; "))
    }
}

fn line_sum(value: &Value) -> Option<f64> {
    line_items(value)?
        .into_iter()
        .map(|l| number_at(l, "amount"))
        .sum()
}

fn plural(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

fn check_subtotal(value: &Value) -> Check {
    let name = "subtotal";
    let (Some(subtotal), Some(sum)) = (number(value, "subtotal"), line_sum(value)) else {
        return fail(name, "subtotal and the line amounts must be numbers");
    };
    if (subtotal - sum).abs() > CENT {
        fail(
            name,
            &format!(
                "subtotal {} is not the sum of the lines {:.2}",
                money(subtotal),
                sum
            ),
        )
    } else {
        pass(
            name,
            &format!("subtotal {} = sum of lines", money(subtotal)),
        )
    }
}

fn check_tax(value: &Value) -> Check {
    let name = "tax";
    let Some(tax) = number(value, "tax") else {
        return fail(name, "tax must be a number");
    };
    let Some(rate) = number(value, "tax_rate") else {
        return pass(
            name,
            "no tax_rate given, so the tax amount is not recomputed",
        );
    };
    let Some(subtotal) = number(value, "subtotal") else {
        return fail(name, "subtotal must be a number");
    };
    let expected = subtotal * rate;
    if (tax - expected).abs() > CENT {
        fail(
            name,
            &format!(
                "tax {} is not subtotal {} x rate {} = {:.2}",
                money(tax),
                money(subtotal),
                rate,
                expected
            ),
        )
    } else {
        pass(name, &format!("tax {} = subtotal x rate", money(tax)))
    }
}

fn check_total(value: &Value) -> Check {
    let name = "total";
    let (Some(total), Some(subtotal), Some(tax)) = (
        number(value, "total"),
        number(value, "subtotal"),
        number(value, "tax"),
    ) else {
        return fail(name, "total, subtotal and tax must be numbers");
    };
    if (total - (subtotal + tax)).abs() > CENT {
        fail(
            name,
            &format!(
                "total {} is not subtotal {} + tax {} = {:.2}",
                money(total),
                money(subtotal),
                money(tax),
                subtotal + tax
            ),
        )
    } else {
        pass(name, &format!("total {} = subtotal + tax", money(total)))
    }
}

fn check_supplier(value: &Value, suppliers: Option<&Suppliers>) -> Check {
    let name = "supplier";
    let Some(list) = suppliers else {
        return pass(
            name,
            "no supplier list given, so the supplier is not checked",
        );
    };
    match lookup(value, "supplier.tax_id").and_then(text_of) {
        None => fail(name, "supplier.tax_id is missing"),
        Some(tax_id) if list.contains(&tax_id) => {
            pass(name, &format!("supplier {tax_id} is approved"))
        }
        Some(tax_id) => fail(
            name,
            &format!("supplier {tax_id} is not on the approved list"),
        ),
    }
}

fn pass(name: &'static str, message: &str) -> Check {
    Check {
        name,
        passed: true,
        message: message.to_string(),
    }
}

fn fail(name: &'static str, message: &str) -> Check {
    Check {
        name,
        passed: false,
        message: message.to_string(),
    }
}

/// A number from a field. Text that holds a number is accepted, so XML works.
fn number(value: &Value, path: &str) -> Option<f64> {
    number_of(lookup(value, path)?)
}

fn number_at(value: &Value, key: &str) -> Option<f64> {
    number_of(value.get(key)?)
}

fn number_of(value: &Value) -> Option<f64> {
    let n = match value {
        Value::Number(n) => n.as_f64()?,
        Value::String(s) => s.trim().parse::<f64>().ok()?,
        _ => return None,
    };
    n.is_finite().then_some(n)
}

fn text_of(value: &Value) -> Option<String> {
    match value {
        Value::String(s) => Some(s.trim().to_string()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn money(n: f64) -> String {
    format!("{n:.2}")
}

impl fmt::Display for Verdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}
