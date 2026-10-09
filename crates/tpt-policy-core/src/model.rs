//! The validated policy model. Produced by [`crate::parse_policy`].

use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A policy outcome. Declared from least to most severe, so `Ord` gives the
/// severity precedence used when several rules match:
/// `rejected > approval_required > review > approved`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Approved,
    Review,
    ApprovalRequired,
    Rejected,
}

impl Decision {
    pub const ALL: [Decision; 4] = [
        Decision::Approved,
        Decision::Review,
        Decision::ApprovalRequired,
        Decision::Rejected,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Decision::Approved => "approved",
            Decision::Review => "review",
            Decision::ApprovalRequired => "approval_required",
            Decision::Rejected => "rejected",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|d| d.as_str() == text)
    }
}

impl fmt::Display for Decision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A parsed policy file.
#[derive(Debug, Clone, PartialEq)]
pub struct Policy {
    pub name: String,
    /// Recorded in every evaluation. `unversioned` when the file omits it.
    pub version: String,
    /// Policy file format. `1` when the file omits it. See [`crate::POLICY_FORMAT`].
    pub format: u32,
    /// Decision when no rule matches. Defaults to `review` (fails safe).
    pub default_decision: Decision,
    pub rules: Vec<Rule>,
    pub tests: Vec<TestCase>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Rule {
    pub id: String,
    pub description: Option<String>,
    pub when: Condition,
    pub then: Outcome,
}

/// What a matching rule contributes to the decision.
#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    pub decision: Decision,
    pub approver: Option<String>,
    pub requirement: Option<String>,
    pub warning: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Condition {
    All(Vec<Condition>),
    Any(Vec<Condition>),
    Not(Box<Condition>),
    Check(Check),
}

/// One field test, such as `expense.amount` greater than `5000`.
#[derive(Debug, Clone, PartialEq)]
pub struct Check {
    /// Dotted path into the input, e.g. `expense.amount` or `items.0.sku`.
    pub path: String,
    pub op: Op,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    Equals(Value),
    NotEquals(Value),
    Gt(Value),
    Gte(Value),
    Lt(Value),
    Lte(Value),
    In(Vec<Value>),
    NotIn(Vec<Value>),
    Contains(Value),
    Exists(bool),
}

impl Op {
    /// Human-readable form, used in explanations.
    pub fn describe(&self) -> String {
        match self {
            Op::Equals(v) => format!("equals {v}"),
            Op::NotEquals(v) => format!("does not equal {v}"),
            Op::Gt(v) => format!("is greater than {v}"),
            Op::Gte(v) => format!("is at least {v}"),
            Op::Lt(v) => format!("is less than {v}"),
            Op::Lte(v) => format!("is at most {v}"),
            Op::In(list) => format!("is one of {}", Value::Array(list.clone())),
            Op::NotIn(list) => format!("is not one of {}", Value::Array(list.clone())),
            Op::Contains(v) => format!("contains {v}"),
            Op::Exists(true) => "exists".to_string(),
            Op::Exists(false) => "is missing".to_string(),
        }
    }
}

/// An inline test defined in the policy file.
#[derive(Debug, Clone, PartialEq)]
pub struct TestCase {
    pub name: String,
    pub input: Value,
    pub expect: Expectation,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Expectation {
    pub decision: Decision,
    /// When present, the matched rules must equal this list exactly, in order.
    pub matched_rules: Option<Vec<String>>,
}
