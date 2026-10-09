//! Checks records against a schema, then optionally against a policy.
//!
//! Order matters. A record is checked against the schema first. Only a record
//! that passes the schema is checked for duplicates, and only a valid record
//! is evaluated against the policy. So a policy decision always describes data
//! that passed the schema.

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::{Map, Value};
use tpt_policy_core::{evaluate, Evaluation, Policy};
use tpt_schema::{check_record, coerce, lookup, Mode, Schema, UniqueIndex};

use crate::input::{set_path, Item};

/// Pseudo-field used when a record could not be read at all.
pub const RECORD_FIELD: &str = "(record)";

/// One problem with one record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RowError {
    pub field: String,
    pub reason: String,
}

/// The result for one record.
#[derive(Debug, Clone)]
pub struct Outcome {
    pub number: u64,
    /// The record as read. `None` when the record could not be read at all.
    pub record: Option<Value>,
    pub errors: Vec<RowError>,
    /// The policy result. Present only for valid records, and only when a policy was given.
    pub evaluation: Option<Evaluation>,
}

impl Outcome {
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }
}

/// Running totals for the summary report. Counts only, no record values.
#[derive(Debug, Default, Serialize)]
pub struct Summary {
    pub processed: u64,
    pub valid: u64,
    pub invalid: u64,
    /// Number of errors per field, for records that failed.
    pub errors_by_field: BTreeMap<String, u64>,
    /// Policy decisions for valid records. Empty when no policy was given.
    pub decisions: BTreeMap<String, u64>,
}

impl Summary {
    pub fn add(&mut self, outcome: &Outcome) {
        self.processed += 1;
        if outcome.is_valid() {
            self.valid += 1;
        } else {
            self.invalid += 1;
            for error in &outcome.errors {
                *self.errors_by_field.entry(error.field.clone()).or_default() += 1;
            }
        }
        if let Some(evaluation) = &outcome.evaluation {
            *self
                .decisions
                .entry(evaluation.decision.to_string())
                .or_default() += 1;
        }
    }
}

/// Runs the checks. Keep one validator per run, because duplicate checks need
/// the values seen so far.
pub struct Validator<'a> {
    schema: &'a Schema,
    policy: Option<&'a Policy>,
    mode: Mode,
    unique: UniqueIndex,
}

impl<'a> Validator<'a> {
    pub fn new(schema: &'a Schema, policy: Option<&'a Policy>, mode: Mode) -> Self {
        Self {
            schema,
            policy,
            mode,
            unique: UniqueIndex::new(),
        }
    }

    /// Check one item from the input.
    pub fn check(&mut self, item: Item) -> Outcome {
        match item {
            Item::Malformed { number, message } => Outcome {
                number,
                record: None,
                errors: vec![RowError {
                    field: RECORD_FIELD.to_string(),
                    reason: message,
                }],
                evaluation: None,
            },
            Item::Row(row) => {
                let mut errors: Vec<RowError> = check_record(self.schema, &row.record, self.mode)
                    .into_iter()
                    .map(|e| RowError {
                        field: e.field,
                        reason: e.reason,
                    })
                    .collect();
                if errors.is_empty() {
                    errors.extend(
                        self.unique
                            .check(self.schema, &row.record, row.number)
                            .into_iter()
                            .map(|e| RowError {
                                field: e.field,
                                reason: e.reason,
                            }),
                    );
                }
                let evaluation = match (errors.is_empty(), self.policy) {
                    (true, Some(policy)) => {
                        let typed = self.typed(&row.record);
                        Some(evaluate(policy, &Value::Object(typed)))
                    }
                    _ => None,
                };
                Outcome {
                    number: row.number,
                    record: Some(row.record),
                    errors,
                    evaluation,
                }
            }
        }
    }

    /// The record with each schema field converted to its declared type.
    /// CSV cells are text, and policy comparisons need numbers and booleans.
    /// Only call this for records that passed the schema, so every field converts.
    fn typed(&self, record: &Value) -> Map<String, Value> {
        let mut typed = record.as_object().cloned().unwrap_or_default();
        for field in &self.schema.fields {
            let Some(value) = lookup(record, &field.path).filter(|v| !v.is_null()) else {
                continue;
            };
            if let Ok(converted) = coerce(field.kind, value, self.mode) {
                set_path(&mut typed, &field.path, converted);
            }
        }
        typed
    }
}
