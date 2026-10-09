//! Approval rules for TPT Approval Engine.
//!
//! An approval file lists rules. Each rule has a `when` test on request fields,
//! and says either that the request is approved automatically, or which role
//! must approve it. The rules are compiled to a `tpt-policy-core` policy, so
//! evaluation, merging and explanations are the same as for TPT App Policy.
//!
//! A request that no rule covers gets `review`. It is never approved by default.

use std::collections::BTreeSet;

use serde::Deserialize;
use serde_json::{json, Map, Value};
use tpt_policy_core::{evaluate, parse_policy, Evaluation, Policy};

/// Version of this crate, recorded in results.
pub const APPROVE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// A parsed approval file.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovalFile {
    pub approval: String,
    #[serde(default = "default_version")]
    pub version: String,
    pub rules: Vec<ApprovalRule>,
}

fn default_version() -> String {
    "1.0.0".to_string()
}

/// One approval rule.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovalRule {
    /// A name for the rule, shown in explanations. Defaults to `rule-N`.
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    /// Field name to test. Each value is a literal, or a range such as `<1000`,
    /// `>=5000`, or `1000-5000` (both ends included).
    pub when: serde_json::Map<String, Value>,
    /// `automatic`: the request is approved without anyone else.
    #[serde(default)]
    pub decision: Option<String>,
    /// The role or roles that must approve.
    #[serde(default)]
    pub approver: Option<Approver>,
    /// Something that must be true before the decision is final.
    #[serde(default)]
    pub requirement: Option<String>,
}

/// Who must approve. Written as `manager`, as `[manager, finance]` (all of them
/// must approve), or as `{ role: manager }`.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Approver {
    Role(String),
    Roles(Vec<String>),
    Named { role: String },
}

impl Approver {
    fn roles(&self) -> Vec<String> {
        match self {
            Self::Role(role) | Self::Named { role } => vec![role.clone()],
            Self::Roles(roles) => roles.clone(),
        }
    }
}

/// One test on one field, after parsing the `when` value.
#[derive(Debug, Clone, PartialEq)]
pub enum Test {
    Lt(f64),
    Lte(f64),
    Gt(f64),
    Gte(f64),
    Equals(Value),
}

/// Parse a file. Errors say what is wrong and how to fix it.
pub fn parse_approval(text: &str) -> Result<ApprovalFile, String> {
    let file: ApprovalFile = serde_yaml::from_str(text).map_err(|e| e.to_string())?;
    validate(&file)?;
    Ok(file)
}

fn validate(file: &ApprovalFile) -> Result<(), String> {
    if file.approval.trim().is_empty() {
        return Err("'approval' is empty; give the approval a name".into());
    }
    if file.rules.is_empty() {
        return Err("the file has no rules; add at least one rule under 'rules:'".into());
    }
    let mut ids = BTreeSet::new();
    for (index, rule) in file.rules.iter().enumerate() {
        let name = rule_name(index, rule);
        let fail = |why: &str| Err(format!("rule '{name}': {why}"));
        if !ids.insert(name.clone()) {
            return fail("the id is used by another rule; give each rule its own id");
        }
        if rule.when.is_empty() {
            return fail("'when' is empty; name at least one field to test");
        }
        match (&rule.decision, &rule.approver) {
            (Some(decision), None) => {
                if decision != "automatic" {
                    return fail(
                        "'decision' can only be 'automatic'; use 'approver' for approvals",
                    );
                }
            }
            (None, Some(approver)) => {
                let roles = approver.roles();
                if roles.is_empty() || roles.iter().any(|r| r.trim().is_empty()) {
                    return fail("'approver' needs at least one role name");
                }
            }
            (None, None) => {
                return fail("set either 'decision: automatic' or 'approver: <role>'");
            }
            (Some(_), Some(_)) => {
                return fail("set either 'decision' or 'approver', not both");
            }
        }
        for (field, value) in &rule.when {
            parse_test(value).map_err(|why| format!("rule '{name}': field '{field}': {why}"))?;
        }
    }
    Ok(())
}

/// The name a rule is known by: its `id`, or `rule-N` counting from 1.
pub fn rule_name(index: usize, rule: &ApprovalRule) -> String {
    rule.id
        .clone()
        .unwrap_or_else(|| format!("rule-{}", index + 1))
}

/// Parse the value of one `when` entry.
///
/// - `<x`, `<=x`, `>x`, `>=x`: a number comparison.
/// - `x-y`: a range, both ends included. Negative numbers work, as in `-5--1`.
/// - `=x` or a plain number: equality. Plain text is compared as text.
pub fn parse_test(value: &Value) -> Result<Vec<Test>, String> {
    let Value::String(text) = value else {
        return Ok(vec![Test::Equals(value.clone())]);
    };
    let s = text.trim();
    let number = |rest: &str| -> Result<f64, String> {
        rest.trim()
            .parse::<f64>()
            .ok()
            .filter(|x| x.is_finite())
            .ok_or_else(|| format!("'{text}' needs a number after the operator"))
    };
    if let Some(rest) = s.strip_prefix("<=") {
        return Ok(vec![Test::Lte(number(rest)?)]);
    }
    if let Some(rest) = s.strip_prefix(">=") {
        return Ok(vec![Test::Gte(number(rest)?)]);
    }
    if let Some(rest) = s.strip_prefix('<') {
        return Ok(vec![Test::Lt(number(rest)?)]);
    }
    if let Some(rest) = s.strip_prefix('>') {
        return Ok(vec![Test::Gt(number(rest)?)]);
    }
    if let Some(rest) = s.strip_prefix('=') {
        return Ok(vec![Test::Equals(equal_value(rest.trim()))]);
    }
    if let Some((low, high)) = split_range(s) {
        if low > high {
            return Err(format!(
                "'{text}' is an empty range; the first number must not be larger"
            ));
        }
        return Ok(vec![Test::Gte(low), Test::Lte(high)]);
    }
    Ok(vec![Test::Equals(equal_value(s))])
}

fn equal_value(text: &str) -> Value {
    match text.parse::<f64>() {
        Ok(x)
            if x.is_finite()
                && text
                    .chars()
                    .all(|c| c.is_ascii_digit() || matches!(c, '-' | '.')) =>
        {
            serde_json::Number::from_f64(x)
                .map_or_else(|| Value::String(text.into()), Value::Number)
        }
        _ => Value::String(text.to_string()),
    }
}

/// `low-high` where both sides are numbers. Tries each dash as the separator, so
/// negative numbers on either side still parse.
fn split_range(s: &str) -> Option<(f64, f64)> {
    for (index, c) in s.char_indices() {
        if c != '-' || index == 0 {
            continue;
        }
        let low = s[..index].trim().parse::<f64>().ok();
        let high = s[index + 1..].trim().parse::<f64>().ok();
        if let (Some(low), Some(high)) = (low, high) {
            if low.is_finite() && high.is_finite() {
                return Some((low, high));
            }
        }
    }
    None
}

/// Compile the approval rules to a policy in `tpt-policy-core` format. A rule
/// with several approvers becomes one policy rule per approver.
pub fn compile(file: &ApprovalFile) -> Result<Policy, String> {
    let mut rules = Vec::new();
    for (index, rule) in file.rules.iter().enumerate() {
        let name = rule_name(index, rule);
        let mut when = Map::new();
        for (field, value) in &rule.when {
            let mut ops = Map::new();
            for test in parse_test(value)? {
                match test {
                    Test::Lt(x) => ops.insert("lt".into(), json!(x)),
                    Test::Lte(x) => ops.insert("lte".into(), json!(x)),
                    Test::Gt(x) => ops.insert("gt".into(), json!(x)),
                    Test::Gte(x) => ops.insert("gte".into(), json!(x)),
                    Test::Equals(v) => ops.insert("equals".into(), v),
                };
            }
            when.insert(field.clone(), Value::Object(ops));
        }
        let outcomes: Vec<(String, Value)> = match &rule.approver {
            None => vec![(name.clone(), json!({ "decision": "approved" }))],
            Some(approver) => {
                let roles = approver.roles();
                let single = roles.len() == 1;
                roles
                    .into_iter()
                    .enumerate()
                    .map(|(n, role)| {
                        let id = if single {
                            name.clone()
                        } else {
                            format!("{name}-{}", n + 1)
                        };
                        (
                            id,
                            json!({ "decision": "approval_required", "approver": role }),
                        )
                    })
                    .collect()
            }
        };
        for (id, mut then) in outcomes {
            if let Some(requirement) = &rule.requirement {
                then["requirement"] = json!(requirement);
            }
            let mut policy_rule = json!({ "id": id, "when": when, "then": then });
            if let Some(description) = &rule.description {
                policy_rule["description"] = json!(description);
            }
            rules.push(policy_rule);
        }
    }
    let document = json!({
        "policy": file.approval,
        "version": file.version,
        "default_decision": "review",
        "rules": rules,
    });
    let text = serde_yaml::to_string(&document).map_err(|e| e.to_string())?;
    parse_policy(&text).map_err(|e| format!("{}\n  why: {}\n  fix: {}", e.what, e.why, e.fix))
}

/// Evaluate one request against the approval rules.
pub fn check(file: &ApprovalFile, request: &Value) -> Result<Evaluation, String> {
    let policy = compile(file)?;
    Ok(evaluate(&policy, request))
}

/// What `validate` found beyond syntax. Warnings mean a gap in the rules. Notes
/// are for information.
#[derive(Debug, Default, PartialEq)]
pub struct Analysis {
    pub warnings: Vec<String>,
    pub notes: Vec<String>,
}

/// Look for gaps in the rules. This works when every rule tests the same single
/// field against numbers. Otherwise it says the check was not done.
pub fn analyse(file: &ApprovalFile) -> Analysis {
    let mut analysis = Analysis::default();
    let mut field: Option<&str> = None;
    let mut tested: Vec<(usize, Vec<Test>, bool)> = Vec::new();
    for (index, rule) in file.rules.iter().enumerate() {
        if rule.when.len() != 1 {
            analysis
                .notes
                .push("gap check skipped: rules test more than one field".into());
            return analysis;
        }
        let (name, value) = rule.when.iter().next().expect("one field");
        if field.is_some_and(|f| f != name.as_str()) {
            analysis
                .notes
                .push("gap check skipped: rules test different fields".into());
            return analysis;
        }
        field = Some(name.as_str());
        let tests = parse_test(value).unwrap_or_default();
        if tests
            .iter()
            .any(|t| matches!(t, Test::Equals(v) if !v.is_number()))
        {
            analysis
                .notes
                .push("gap check skipped: text values are not compared for gaps".into());
            return analysis;
        }
        tested.push((index, tests, rule.decision.is_some()));
    }
    let Some(field) = field else {
        return analysis;
    };

    // The rules only change at their boundaries. Testing each boundary, each
    // gap between boundaries, and one point beyond each end covers every case.
    let mut bounds: Vec<f64> = tested
        .iter()
        .flat_map(|(_, tests, _)| tests.iter().map(test_number))
        .collect();
    bounds.sort_by(|a, b| a.partial_cmp(b).expect("finite"));
    bounds.dedup();
    let mut points: Vec<(String, f64)> = Vec::new();
    points.push((format!("below {}", bounds[0]), bounds[0] - 1.0));
    for pair in bounds.windows(2) {
        points.push((
            format!("strictly between {} and {}", pair[0], pair[1]),
            (pair[0] + pair[1]) / 2.0,
        ));
    }
    for &b in &bounds {
        points.push((format!("exactly {b}"), b));
    }
    points.push((
        format!("above {}", bounds[bounds.len() - 1]),
        bounds[bounds.len() - 1] + 1.0,
    ));

    let mut gaps = BTreeSet::new();
    let mut overlaps = BTreeSet::new();
    for (label, x) in &points {
        let matching: Vec<&(usize, Vec<Test>, bool)> = tested
            .iter()
            .filter(|(_, tests, _)| tests.iter().all(|t| holds(t, *x)))
            .collect();
        if matching.is_empty() {
            gaps.insert(format!(
                "no rule covers {field} {label}; requests in that range get review"
            ));
        }
        let automatic = matching.iter().any(|(_, _, auto)| *auto);
        let approval = matching.iter().any(|(_, _, auto)| !*auto);
        if automatic && approval {
            overlaps.insert(format!(
                "{field} {label}: a rule approves automatically and another requires approval; approval is required"
            ));
        }
    }
    analysis.warnings.extend(gaps);
    analysis.notes.extend(overlaps);
    analysis
}

fn test_number(test: &Test) -> f64 {
    match test {
        Test::Lt(x) | Test::Lte(x) | Test::Gt(x) | Test::Gte(x) => *x,
        Test::Equals(v) => v.as_f64().expect("numeric test"),
    }
}

fn holds(test: &Test, x: f64) -> bool {
    match test {
        Test::Lt(v) => x < *v,
        Test::Lte(v) => x <= *v,
        Test::Gt(v) => x > *v,
        Test::Gte(v) => x >= *v,
        Test::Equals(v) => v.as_f64() == Some(x),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const EXPENSE: &str = "approval: expense\nrules:\n  - when:\n      amount: \"<1000\"\n    decision: automatic\n  - when:\n      amount: \"1000-5000\"\n    approver:\n      role: manager\n  - when:\n      amount: \">5000\"\n    approver: director\n    requirement: Quote attached\n";

    #[test]
    fn ranges_parse_including_negative_numbers() {
        assert_eq!(parse_test(&json!("<1000")).unwrap(), vec![Test::Lt(1000.0)]);
        assert_eq!(parse_test(&json!(">=5")).unwrap(), vec![Test::Gte(5.0)]);
        assert_eq!(
            parse_test(&json!("1000-5000")).unwrap(),
            vec![Test::Gte(1000.0), Test::Lte(5000.0)]
        );
        assert_eq!(
            parse_test(&json!("-5--1")).unwrap(),
            vec![Test::Gte(-5.0), Test::Lte(-1.0)]
        );
    }

    #[test]
    fn text_that_looks_like_a_date_is_equality() {
        assert_eq!(
            parse_test(&json!("2026-01-01")).unwrap(),
            vec![Test::Equals(json!("2026-01-01"))]
        );
    }

    #[test]
    fn operator_without_a_number_is_an_error() {
        assert!(parse_test(&json!("<abc"))
            .unwrap_err()
            .contains("needs a number"));
    }

    #[test]
    fn empty_range_is_an_error() {
        assert!(parse_test(&json!("5000-1000"))
            .unwrap_err()
            .contains("empty range"));
    }

    #[test]
    fn rule_needs_decision_or_approver_but_not_both() {
        let neither = "approval: a\nrules:\n  - when: {amount: \"<1\"}\n";
        assert!(parse_approval(neither)
            .unwrap_err()
            .contains("either 'decision"));
        let both = "approval: a\nrules:\n  - when: {amount: \"<1\"}\n    decision: automatic\n    approver: x\n";
        assert!(parse_approval(both).unwrap_err().contains("not both"));
    }

    #[test]
    fn decision_other_than_automatic_is_refused() {
        let text = "approval: a\nrules:\n  - when: {amount: \"<1\"}\n    decision: approved\n";
        assert!(parse_approval(text)
            .unwrap_err()
            .contains("can only be 'automatic'"));
    }

    #[test]
    fn duplicate_ids_are_refused() {
        let text = "approval: a\nrules:\n  - id: x\n    when: {amount: \"<1\"}\n    decision: automatic\n  - id: x\n    when: {amount: \">1\"}\n    decision: automatic\n";
        assert!(
            parse_approval(text).unwrap_err().contains("already")
                || parse_approval(text)
                    .unwrap_err()
                    .contains("used by another")
        );
    }

    #[test]
    fn expense_rules_give_the_expected_decisions() {
        let file = parse_approval(EXPENSE).unwrap();
        let small = check(&file, &json!({"amount": 500})).unwrap();
        assert_eq!(small.decision.to_string(), "approved");
        let mid = check(&file, &json!({"amount": 3000})).unwrap();
        assert_eq!(mid.decision.to_string(), "approval_required");
        assert_eq!(mid.approvers, vec!["manager"]);
        let large = check(&file, &json!({"amount": 9000})).unwrap();
        assert_eq!(large.approvers, vec!["director"]);
        assert_eq!(large.requirements, vec!["Quote attached"]);
    }

    #[test]
    fn band_edges_are_on_the_right_side() {
        let file = parse_approval(EXPENSE).unwrap();
        // 1000 is in the manager band; 5000 is in the manager band; 5000.01 is director.
        assert_eq!(
            check(&file, &json!({"amount": 1000})).unwrap().approvers,
            vec!["manager"]
        );
        assert_eq!(
            check(&file, &json!({"amount": 5000})).unwrap().approvers,
            vec!["manager"]
        );
        assert_eq!(
            check(&file, &json!({"amount": 5000.01})).unwrap().approvers,
            vec!["director"]
        );
    }

    #[test]
    fn missing_field_gets_review_not_approval() {
        let file = parse_approval(EXPENSE).unwrap();
        let result = check(&file, &json!({"department": "sales"})).unwrap();
        assert_eq!(result.decision.to_string(), "review");
    }

    #[test]
    fn several_approvers_all_appear() {
        let text =
            "approval: a\nrules:\n  - when: {amount: \">0\"}\n    approver: [manager, finance]\n";
        let file = parse_approval(text).unwrap();
        let result = check(&file, &json!({"amount": 10})).unwrap();
        assert_eq!(result.decision.to_string(), "approval_required");
        assert_eq!(result.approvers, vec!["manager", "finance"]);
    }

    #[test]
    fn analysis_finds_no_gap_in_the_expense_rules() {
        let file = parse_approval(EXPENSE).unwrap();
        assert_eq!(analyse(&file), Analysis::default());
    }

    #[test]
    fn analysis_reports_a_gap_between_bands() {
        let text = "approval: a\nrules:\n  - when: {amount: \"<1000\"}\n    decision: automatic\n  - when: {amount: \">2000\"}\n    approver: director\n";
        let file = parse_approval(text).unwrap();
        let analysis = analyse(&file);
        assert!(
            analysis
                .warnings
                .iter()
                .any(|w| w.contains("strictly between 1000 and 2000")),
            "{analysis:?}"
        );
    }

    #[test]
    fn analysis_notes_automatic_and_approval_overlap() {
        let text = "approval: a\nrules:\n  - when: {amount: \"<=2000\"}\n    decision: automatic\n  - when: {amount: \">1000\"}\n    approver: manager\n";
        let file = parse_approval(text).unwrap();
        let analysis = analyse(&file);
        assert!(
            analysis
                .notes
                .iter()
                .any(|n| n.contains("approval is required")),
            "{analysis:?}"
        );
        assert!(analysis.warnings.is_empty());
    }

    #[test]
    fn analysis_skips_multi_field_rules_and_says_so() {
        let text = "approval: a\nrules:\n  - when: {amount: \"<1\", region: NZ}\n    decision: automatic\n";
        let file = parse_approval(text).unwrap();
        let analysis = analyse(&file);
        assert!(analysis
            .notes
            .iter()
            .any(|n| n.contains("more than one field")));
    }
}
