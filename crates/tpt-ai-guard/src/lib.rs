//! Action policy for TPT AI Action Guard.
//!
//! An AI agent asks to do an action, such as `refund_customer` with an amount.
//! The guard decides ALLOW, DENY or REQUIRE_APPROVAL from deterministic rules.
//! It never runs the action itself. The caller does, and only after ALLOW.
//!
//! Rules compile to a `tpt-policy-core` policy. When several rules match, the
//! most restrictive decision wins: DENY, then REQUIRE_APPROVAL, then ALLOW. An
//! action that no rule covers is denied.

use std::collections::BTreeSet;

use serde::Deserialize;
use serde_json::{json, Map, Value};
use tpt_policy_core::{evaluate, parse_policy, Decision, Evaluation, Policy};

/// Version of this crate, recorded in results.
pub const GUARD_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The guard's answer for one action request.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Verdict {
    /// `ALLOW`, `DENY` or `REQUIRE_APPROVAL`.
    pub decision: &'static str,
    /// Why, in words. Taken from the rules that produced the decision.
    pub reason: String,
    pub action: String,
    pub matched_rules: Vec<String>,
    pub policy: PolicyRef,
    /// A fingerprint of the request. The request itself is not stored in results.
    pub input_sha256: String,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct PolicyRef {
    pub name: String,
    pub version: String,
}

/// A parsed action policy.
#[derive(Debug, Clone)]
pub struct ActionPolicy {
    pub name: String,
    pub version: String,
    pub rules: Vec<ActionRule>,
}

/// A rule as written. Either a bare list of rules, or a named document with a
/// `rules` list. Both are accepted.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum PolicyDocument {
    Bare(Vec<ActionRule>),
    Named {
        #[serde(default = "default_name")]
        name: String,
        #[serde(default = "default_version")]
        version: String,
        rules: Vec<ActionRule>,
    },
}

fn default_name() -> String {
    "action-policy".to_string()
}

fn default_version() -> String {
    "1.0.0".to_string()
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionRule {
    #[serde(default)]
    pub id: Option<String>,
    /// The action this rule is about, such as `refund_customer`.
    pub action: String,
    /// Optional conditions on the request, in `tpt-policy` form, such as `amount: {gt: 5000}`.
    #[serde(default)]
    pub when: Map<String, Value>,
    pub decision: RuleDecision,
    /// Shown as the reason when this rule decides the outcome.
    #[serde(default)]
    pub reason: Option<String>,
}

/// The decision a rule gives. Written as `allow`, `deny` or `require_approval`,
/// or as `{ require_approval: true }` (the same forms, as a map).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum RuleDecision {
    Name(ActionDecision),
    Map(MapDecision),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionDecision {
    Allow,
    Deny,
    RequireApproval,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapDecision {
    #[serde(default)]
    pub allow: Option<bool>,
    #[serde(default)]
    pub deny: Option<bool>,
    #[serde(default)]
    pub require_approval: Option<bool>,
}

impl RuleDecision {
    fn resolve(self) -> Result<ActionDecision, String> {
        match self {
            Self::Name(d) => Ok(d),
            Self::Map(m) => {
                let chosen: Vec<ActionDecision> = [
                    (m.allow == Some(true), ActionDecision::Allow),
                    (m.deny == Some(true), ActionDecision::Deny),
                    (
                        m.require_approval == Some(true),
                        ActionDecision::RequireApproval,
                    ),
                ]
                .into_iter()
                .filter(|(set, _)| *set)
                .map(|(_, d)| d)
                .collect();
                match chosen.as_slice() {
                    [one] => Ok(*one),
                    _ => Err("'decision' map must set exactly one of allow, deny or require_approval to true".into()),
                }
            }
        }
    }
}

/// Parse and check a policy file.
pub fn parse_action_policy(text: &str) -> Result<ActionPolicy, String> {
    let document: PolicyDocument = serde_yaml::from_str(text).map_err(|e| e.to_string())?;
    let (name, version, rules) = match document {
        PolicyDocument::Bare(rules) => ("action-policy".to_string(), default_version(), rules),
        PolicyDocument::Named {
            name,
            version,
            rules,
        } => (name, version, rules),
    };
    if rules.is_empty() {
        return Err("the policy has no rules; add at least one under 'rules:'".into());
    }
    if name.trim().is_empty() {
        return Err("the policy name is empty".into());
    }
    let mut ids = BTreeSet::new();
    for (index, rule) in rules.iter().enumerate() {
        let id = rule_name(index, rule);
        if rule.action.trim().is_empty() {
            return Err(format!(
                "rule '{id}': 'action' is empty; name the action it covers"
            ));
        }
        if !ids.insert(id.clone()) {
            return Err(format!(
                "rule id '{id}' is used twice; give each rule its own id"
            ));
        }
        rule.decision
            .resolve()
            .map_err(|why| format!("rule '{id}': {why}"))?;
    }
    Ok(ActionPolicy {
        name,
        version,
        rules,
    })
}

fn rule_name(index: usize, rule: &ActionRule) -> String {
    rule.id
        .clone()
        .unwrap_or_else(|| format!("rule-{}", index + 1))
}

/// Compile to a `tpt-policy-core` policy. Actions no rule covers are denied.
pub fn compile(policy: &ActionPolicy) -> Result<Policy, String> {
    let mut rules = Vec::new();
    for (index, rule) in policy.rules.iter().enumerate() {
        let id = rule_name(index, rule);
        let mut when = rule.when.clone();
        if when.contains_key("action") {
            return Err(format!(
                "rule '{id}': 'action' cannot appear under 'when'; use the 'action' key"
            ));
        }
        when.insert("action".into(), json!({ "equals": rule.action }));
        let decision = match rule.decision.resolve()? {
            ActionDecision::Allow => "approved",
            ActionDecision::Deny => "rejected",
            ActionDecision::RequireApproval => "approval_required",
        };
        let mut policy_rule = json!({ "id": id, "when": when, "then": { "decision": decision } });
        if let Some(reason) = &rule.reason {
            policy_rule["description"] = json!(reason);
        }
        rules.push(policy_rule);
    }
    let document = json!({
        "policy": policy.name,
        "version": policy.version,
        "default_decision": "rejected",
        "rules": rules,
    });
    let text = serde_yaml::to_string(&document).map_err(|e| e.to_string())?;
    parse_policy(&text).map_err(|e| format!("{}: {}", e.what, e.why))
}

/// Decide one action request. The request must be a JSON object with a string `action`.
pub fn decide(policy: &ActionPolicy, request: &Value) -> Result<Verdict, String> {
    let action = request
        .get("action")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            "the request needs a string field 'action', such as \"refund_customer\"".to_string()
        })?
        .to_string();
    let compiled = compile(policy)?;
    let evaluation: Evaluation = evaluate(&compiled, request);
    let decision = match evaluation.decision {
        Decision::Approved => "ALLOW",
        Decision::ApprovalRequired => "REQUIRE_APPROVAL",
        // Review cannot be produced by a compiled policy. Treat it as a denial if it ever is.
        Decision::Review | Decision::Rejected => "DENY",
    };
    let reasons: Vec<String> = evaluation
        .explanations
        .iter()
        .filter(|e| e.decision == evaluation.decision)
        .map(|e| e.message.clone())
        .collect();
    let reason = if evaluation.matched_rules.is_empty() {
        format!("no rule allows action '{action}'; denied by default")
    } else {
        unique(reasons).join("; ")
    };
    Ok(Verdict {
        decision,
        reason,
        action,
        matched_rules: evaluation.matched_rules.clone(),
        policy: PolicyRef {
            name: evaluation.policy.name.clone(),
            version: evaluation.policy.version.clone(),
        },
        input_sha256: evaluation.input_sha256.clone(),
    })
}

fn unique(items: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    items
        .into_iter()
        .filter(|s| seen.insert(s.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEC_STYLE: &str = "- action: refund_customer\n  when:\n    amount:\n      gt: 5000\n  decision:\n    require_approval: true\n  reason: refund exceeds $5,000 limit\n- action: refund_customer\n  when:\n    amount:\n      lte: 5000\n  decision: allow\n  reason: refund within limit\n- action: delete_customer\n  decision: deny\n  reason: deletion is for the data team\n";

    fn policy() -> ActionPolicy {
        parse_action_policy(SPEC_STYLE).unwrap()
    }

    #[test]
    fn spec_example_requires_approval_with_the_reason() {
        let v = decide(
            &policy(),
            &json!({"action": "refund_customer", "amount": 7500}),
        )
        .unwrap();
        assert_eq!(v.decision, "REQUIRE_APPROVAL");
        assert_eq!(v.reason, "refund exceeds $5,000 limit");
    }

    #[test]
    fn small_refund_is_allowed() {
        let v = decide(
            &policy(),
            &json!({"action": "refund_customer", "amount": 40}),
        )
        .unwrap();
        assert_eq!(v.decision, "ALLOW");
        assert_eq!(v.reason, "refund within limit");
    }

    #[test]
    fn exactly_the_limit_is_allowed_and_one_cent_over_needs_approval() {
        assert_eq!(
            decide(
                &policy(),
                &json!({"action": "refund_customer", "amount": 5000})
            )
            .unwrap()
            .decision,
            "ALLOW"
        );
        assert_eq!(
            decide(
                &policy(),
                &json!({"action": "refund_customer", "amount": 5000.01})
            )
            .unwrap()
            .decision,
            "REQUIRE_APPROVAL"
        );
    }

    #[test]
    fn deny_rule_is_denied() {
        let v = decide(&policy(), &json!({"action": "delete_customer"})).unwrap();
        assert_eq!(v.decision, "DENY");
        assert_eq!(v.reason, "deletion is for the data team");
    }

    #[test]
    fn unknown_action_is_denied_by_default() {
        let v = decide(&policy(), &json!({"action": "wire_money", "amount": 1})).unwrap();
        assert_eq!(v.decision, "DENY");
        assert!(v.reason.contains("denied by default"), "{}", v.reason);
    }

    #[test]
    fn missing_amount_does_not_match_an_amount_rule() {
        // Without an amount, neither refund rule matches, so the refund is denied.
        let v = decide(&policy(), &json!({"action": "refund_customer"})).unwrap();
        assert_eq!(v.decision, "DENY");
    }

    #[test]
    fn most_restrictive_rule_wins_when_several_match() {
        let text = "rules:\n  - action: send\n    decision: allow\n    reason: sending is allowed\n  - action: send\n    decision: require_approval\n    reason: sends need approval\n  - action: send\n    decision: deny\n    reason: sends to this address are blocked\n";
        let p = parse_action_policy(text).unwrap();
        let v = decide(&p, &json!({"action": "send"})).unwrap();
        assert_eq!(v.decision, "DENY");
        assert_eq!(v.reason, "sends to this address are blocked");
    }

    #[test]
    fn approval_beats_allow_and_reports_only_the_approval_reason() {
        let text = "rules:\n  - action: send\n    decision: allow\n    reason: sending is allowed\n  - action: send\n    decision: require_approval\n    reason: sends need approval\n";
        let p = parse_action_policy(text).unwrap();
        let v = decide(&p, &json!({"action": "send"})).unwrap();
        assert_eq!(v.decision, "REQUIRE_APPROVAL");
        assert_eq!(v.reason, "sends need approval");
    }

    #[test]
    fn decision_map_needs_exactly_one_true_value() {
        let text = "rules:\n  - action: x\n    decision:\n      allow: true\n      deny: true\n";
        assert!(parse_action_policy(text)
            .unwrap_err()
            .contains("exactly one"));
    }

    #[test]
    fn unknown_decision_name_is_refused() {
        let text = "rules:\n  - action: x\n    decision: maybe\n";
        assert!(parse_action_policy(text).is_err());
    }

    #[test]
    fn request_without_action_is_refused() {
        assert!(decide(&policy(), &json!({"amount": 1}))
            .unwrap_err()
            .contains("'action'"));
    }

    #[test]
    fn action_key_under_when_is_refused() {
        let text = "rules:\n  - action: x\n    when:\n      action: y\n    decision: allow\n";
        let p = parse_action_policy(text).unwrap();
        assert!(compile(&p)
            .unwrap_err()
            .contains("cannot appear under 'when'"));
    }

    #[test]
    fn verdict_does_not_contain_the_request() {
        let v = decide(
            &policy(),
            &json!({"action": "refund_customer", "amount": 7500, "card": "4111111111111111"}),
        )
        .unwrap();
        let text = serde_json::to_string(&v).unwrap();
        assert!(!text.contains("4111"));
    }
}
