//! Evaluates a policy against JSON input.
//!
//! Rules are checked in file order. The final decision is the most severe
//! decision among matched rules, or the policy's default when none match.
//! Missing fields make a check fail, except `exists: false`.

use std::cmp::Ordering;

use serde::Serialize;
use serde_json::Value;

use crate::model::{Check, Condition, Decision, Op, Policy};
use crate::ENGINE_VERSION;

/// The result of evaluating one input. Serialises to the JSON output format.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Evaluation {
    pub decision: Decision,
    pub approvers: Vec<String>,
    pub requirements: Vec<String>,
    pub matched_rules: Vec<String>,
    pub failed_rules: Vec<FailedRule>,
    pub explanations: Vec<Explanation>,
    pub warnings: Vec<String>,
    pub policy: PolicyRef,
    pub engine_version: &'static str,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PolicyRef {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FailedRule {
    pub rule: String,
    /// The first check that failed, e.g. `expense.amount is greater than 5000 (actual: 500)`.
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Explanation {
    pub rule: String,
    pub decision: Decision,
    pub message: String,
}

/// Evaluate `policy` against `input`.
pub fn evaluate(policy: &Policy, input: &Value) -> Evaluation {
    let mut matched = Vec::new();
    let mut failed = Vec::new();
    for rule in &policy.rules {
        match check_condition(&rule.when, input) {
            Ok(()) => matched.push(rule),
            Err(reason) => failed.push(FailedRule {
                rule: rule.id.clone(),
                reason,
            }),
        }
    }

    let decision = matched
        .iter()
        .map(|rule| rule.then.decision)
        .max()
        .unwrap_or(policy.default_decision);

    Evaluation {
        decision,
        approvers: unique(matched.iter().filter_map(|r| r.then.approver.clone())),
        requirements: unique(matched.iter().filter_map(|r| r.then.requirement.clone())),
        matched_rules: matched.iter().map(|r| r.id.clone()).collect(),
        failed_rules: failed,
        explanations: matched
            .iter()
            .map(|rule| Explanation {
                rule: rule.id.clone(),
                decision: rule.then.decision,
                message: rule
                    .description
                    .clone()
                    .unwrap_or_else(|| format!("rule '{}' matched", rule.id)),
            })
            .collect(),
        warnings: unique(matched.iter().filter_map(|r| r.then.warning.clone())),
        policy: PolicyRef {
            name: policy.name.clone(),
            version: policy.version.clone(),
        },
        engine_version: ENGINE_VERSION,
    }
}

/// Returns `Err(reason)` on the first failed check.
pub(crate) fn check_condition(condition: &Condition, input: &Value) -> Result<(), String> {
    match condition {
        Condition::All(items) => {
            for item in items {
                check_condition(item, input)?;
            }
            Ok(())
        }
        Condition::Any(items) => {
            let mut reasons = Vec::new();
            for item in items {
                match check_condition(item, input) {
                    Ok(()) => return Ok(()),
                    Err(reason) => reasons.push(reason),
                }
            }
            Err(format!(
                "none of the alternatives matched: {}",
                reasons.join("; ")
            ))
        }
        Condition::Not(inner) => match check_condition(inner, input) {
            Ok(()) => Err("a 'not' condition matched".to_string()),
            Err(_) => Ok(()),
        },
        Condition::Check(check) => check_field(check, input),
    }
}

fn check_field(check: &Check, input: &Value) -> Result<(), String> {
    // Null counts as missing.
    let actual = resolve(input, &check.path).filter(|v| !v.is_null());

    if let Op::Exists(want) = check.op {
        return if actual.is_some() == want {
            Ok(())
        } else {
            Err(format!("{} {}", check.path, check.op.describe()))
        };
    }

    let Some(actual) = actual else {
        return Err(format!("{} is missing", check.path));
    };

    let holds = match &check.op {
        Op::Equals(v) => values_equal(actual, v),
        Op::NotEquals(v) => !values_equal(actual, v),
        Op::Gt(v) => compare(actual, v) == Some(Ordering::Greater),
        Op::Gte(v) => matches!(
            compare(actual, v),
            Some(Ordering::Greater | Ordering::Equal)
        ),
        Op::Lt(v) => compare(actual, v) == Some(Ordering::Less),
        Op::Lte(v) => matches!(compare(actual, v), Some(Ordering::Less | Ordering::Equal)),
        Op::In(list) => list.iter().any(|v| values_equal(actual, v)),
        Op::NotIn(list) => !list.iter().any(|v| values_equal(actual, v)),
        Op::Contains(v) => match actual {
            Value::Array(items) => items.iter().any(|item| values_equal(item, v)),
            Value::String(text) => v.as_str().is_some_and(|needle| text.contains(needle)),
            _ => false,
        },
        // Handled above.
        Op::Exists(_) => true,
    };

    if holds {
        Ok(())
    } else {
        Err(format!(
            "{} {} (actual: {})",
            check.path,
            check.op.describe(),
            actual
        ))
    }
}

/// Follows a dotted path. Numeric segments index into arrays.
fn resolve<'a>(input: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.')
        .try_fold(input, |current, segment| match current {
            Value::Object(map) => map.get(segment),
            Value::Array(items) => segment.parse::<usize>().ok().and_then(|i| items.get(i)),
            _ => None,
        })
}

/// Numbers compare by value, so `5000` equals `5000.0`.
fn values_equal(a: &Value, b: &Value) -> bool {
    match (a.as_f64(), b.as_f64()) {
        (Some(x), Some(y)) => x == y,
        _ => a == b,
    }
}

/// Orders two numbers, or two strings. Other combinations are unordered.
fn compare(a: &Value, b: &Value) -> Option<Ordering> {
    match (a.as_f64(), b.as_f64()) {
        (Some(x), Some(y)) => x.partial_cmp(&y),
        _ => match (a.as_str(), b.as_str()) {
            (Some(x), Some(y)) => Some(x.cmp(y)),
            _ => None,
        },
    }
}

fn unique(items: impl Iterator<Item = String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for item in items {
        if !out.contains(&item) {
            out.push(item);
        }
    }
    out
}
