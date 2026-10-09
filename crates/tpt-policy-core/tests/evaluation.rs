use serde_json::json;
use tpt_policy_core::{evaluate, parse_policy, run_tests, Decision, Evaluation};

fn eval(policy_yaml: &str, input: serde_json::Value) -> Evaluation {
    let policy = parse_policy(policy_yaml).expect("policy is valid");
    evaluate(&policy, &input)
}

fn parse_err(policy_yaml: &str) -> (String, String) {
    let e = parse_policy(policy_yaml).expect_err("policy should be invalid");
    (e.what, e.location)
}

const BASE: &str = r#"
policy: test
rules:
  - id: big
    when:
      amount:
        gt: 5000
    then:
      decision: approval_required
"#;

#[test]
fn numeric_greater_than_matches_and_rejects_boundary() {
    assert_eq!(
        eval(BASE, json!({"amount": 6000})).decision,
        Decision::ApprovalRequired
    );
    // Strictly greater: exactly 5000 does not match, so the default applies.
    assert_eq!(
        eval(BASE, json!({"amount": 5000})).decision,
        Decision::Review
    );
}

#[test]
fn integer_and_float_compare_equal() {
    let p = r#"
policy: t
rules:
  - id: exact
    when:
      amount: 5000
    then:
      decision: rejected
"#;
    assert_eq!(
        eval(p, json!({"amount": 5000.0})).decision,
        Decision::Rejected
    );
}

#[test]
fn missing_field_fails_check_and_reports_it() {
    let ev = eval(BASE, json!({}));
    assert_eq!(ev.decision, Decision::Review);
    assert_eq!(ev.failed_rules[0].reason, "amount is missing");
}

#[test]
fn null_counts_as_missing() {
    assert_eq!(
        eval(BASE, json!({"amount": null})).decision,
        Decision::Review
    );
}

#[test]
fn nested_path_and_shorthand_equals() {
    let p = r#"
policy: t
rules:
  - id: blocked
    when:
      supplier.status: blocked
    then:
      decision: rejected
"#;
    assert_eq!(
        eval(p, json!({"supplier": {"status": "blocked"}})).decision,
        Decision::Rejected
    );
    assert_eq!(
        eval(p, json!({"supplier": {"status": "active"}})).decision,
        Decision::Review
    );
}

#[test]
fn array_index_in_path() {
    let p = r#"
policy: t
rules:
  - id: first-big
    when:
      items.0.price:
        gt: 100
    then:
      decision: approval_required
"#;
    assert_eq!(
        eval(p, json!({"items": [{"price": 150}]})).decision,
        Decision::ApprovalRequired
    );
}

#[test]
fn severity_precedence_and_merged_approvers() {
    let p = r#"
policy: t
rules:
  - id: needs-manager
    when:
      amount: {gt: 100}
    then:
      decision: approval_required
      approver: manager
  - id: blocked
    when:
      flagged: true
    then:
      decision: rejected
  - id: review-note
    when:
      amount: {gt: 50}
    then:
      decision: review
      warning: amount is unusually high
  - id: needs-manager-again
    when:
      amount: {gt: 100}
    then:
      decision: approved
      approver: manager
"#;
    let ev = eval(p, json!({"amount": 500, "flagged": true}));
    assert_eq!(ev.decision, Decision::Rejected);
    assert_eq!(
        ev.matched_rules,
        vec![
            "needs-manager",
            "blocked",
            "review-note",
            "needs-manager-again"
        ]
    );
    // Approver appears once even when two rules name it.
    assert_eq!(ev.approvers, vec!["manager"]);
    assert_eq!(ev.warnings, vec!["amount is unusually high"]);
}

#[test]
fn review_outranks_approved_but_not_approval_required() {
    let p = r#"
policy: t
rules:
  - id: ok
    when: {x: 1}
    then: {decision: approved}
  - id: check
    when: {x: 1}
    then: {decision: review}
"#;
    assert_eq!(eval(p, json!({"x": 1})).decision, Decision::Review);
}

#[test]
fn any_all_not_combinators() {
    let p = r#"
policy: t
rules:
  - id: complex
    when:
      all:
        - any:
            - region: {equals: NZ}
            - region: {equals: AU}
        - not:
            status: {equals: closed}
    then:
      decision: approval_required
"#;
    assert_eq!(
        eval(p, json!({"region": "AU", "status": "open"})).decision,
        Decision::ApprovalRequired
    );
    assert_eq!(
        eval(p, json!({"region": "US", "status": "open"})).decision,
        Decision::Review
    );
    assert_eq!(
        eval(p, json!({"region": "NZ", "status": "closed"})).decision,
        Decision::Review
    );
}

#[test]
fn sibling_keys_are_anded() {
    let p = r#"
policy: t
rules:
  - id: both
    when:
      amount: {gte: 1000, lt: 5000}
    then:
      decision: approval_required
"#;
    assert_eq!(
        eval(p, json!({"amount": 2000})).decision,
        Decision::ApprovalRequired
    );
    assert_eq!(eval(p, json!({"amount": 6000})).decision, Decision::Review);
}

#[test]
fn in_not_in_contains_exists() {
    let p = r#"
policy: t
rules:
  - id: country
    when:
      country: {in: [NZ, AU]}
    then: {decision: approved}
  - id: tags
    when:
      tags: {contains: urgent}
    then: {decision: approval_required}
  - id: text
    when:
      note: {contains: refund}
    then: {decision: rejected}
  - id: has-ref
    when:
      ref: {exists: true}
    then: {decision: approved}
  - id: no-ref
    when:
      ref: {exists: false}
    then: {decision: review}
"#;
    let ev = eval(
        p,
        json!({"country": "NZ", "tags": ["urgent", "x"], "note": "full refund", "ref": "A1"}),
    );
    assert_eq!(ev.matched_rules, vec!["country", "tags", "text", "has-ref"]);
    assert_eq!(ev.decision, Decision::Rejected);

    let ev = eval(p, json!({"country": "US"}));
    assert_eq!(ev.matched_rules, vec!["no-ref"]);
    assert_eq!(ev.decision, Decision::Review);
}

#[test]
fn not_in_excludes_listed_values() {
    let p = r#"
policy: t
rules:
  - id: allowed
    when:
      currency: {not_in: [XXX, BTC]}
    then: {decision: approved}
"#;
    assert_eq!(
        eval(p, json!({"currency": "NZD"})).decision,
        Decision::Approved
    );
    assert_eq!(
        eval(p, json!({"currency": "BTC"})).decision,
        Decision::Review
    );
}

#[test]
fn default_decision_is_used_when_nothing_matches() {
    let p = r#"
policy: t
default_decision: approved
rules:
  - id: never
    when: {x: 999}
    then: {decision: rejected}
"#;
    let ev = eval(p, json!({"x": 1}));
    assert_eq!(ev.decision, Decision::Approved);
    assert!(ev.matched_rules.is_empty());
}

#[test]
fn evaluation_is_deterministic() {
    let policy =
        parse_policy(include_str!("../../../examples/purchasing/purchasing.yaml")).unwrap();
    let input = json!({"amount": 2000});
    let first = serde_json::to_string(&evaluate(&policy, &input)).unwrap();
    for _ in 0..50 {
        assert_eq!(
            serde_json::to_string(&evaluate(&policy, &input)).unwrap(),
            first
        );
    }
}

#[test]
fn inline_tests_report_pass_and_fail() {
    let p = r#"
policy: t
rules:
  - id: big
    when: {amount: {gt: 10}}
    then: {decision: approval_required}
tests:
  - name: passes
    input: {amount: 50}
    expect: {decision: approval_required, matched_rules: [big]}
  - name: wrong decision
    input: {amount: 5}
    expect: {decision: approved}
"#;
    let policy = parse_policy(p).unwrap();
    let outcomes = run_tests(&policy);
    assert!(outcomes[0].passed);
    assert!(!outcomes[1].passed);
    assert_eq!(
        outcomes[1].message,
        "expected decision approved but got review"
    );
}

#[test]
fn validation_unknown_operator() {
    let (what, loc) = parse_err(
        r#"
policy: t
rules:
  - id: r
    when: {amount: {greater_than: 1}}
    then: {decision: approved}
"#,
    );
    assert_eq!(what, "unknown operator");
    assert_eq!(loc, "rules[0].when.amount.greater_than");
}

#[test]
fn validation_duplicate_rule_id() {
    let (what, loc) = parse_err(
        r#"
policy: t
rules:
  - id: r
    when: {x: 1}
    then: {decision: approved}
  - id: r
    when: {x: 2}
    then: {decision: approved}
"#,
    );
    assert_eq!(what, "duplicate rule id");
    assert_eq!(loc, "rules[1].id");
}

#[test]
fn validation_unknown_decision() {
    let (what, loc) = parse_err(
        r#"
policy: t
rules:
  - id: r
    when: {x: 1}
    then: {decision: maybe}
"#,
    );
    assert_eq!(what, "unknown decision");
    assert_eq!(loc, "rules[0].then.decision");
}

#[test]
fn validation_missing_then() {
    let (what, loc) = parse_err(
        r#"
policy: t
rules:
  - id: r
    when: {x: 1}
"#,
    );
    assert_eq!(what, "missing key");
    assert_eq!(loc, "rules[0]");
}

#[test]
fn validation_unknown_top_level_key_catches_typos() {
    let (what, loc) = parse_err(
        r#"
policy: t
rule:
  - id: r
    when: {x: 1}
    then: {decision: approved}
"#,
    );
    assert_eq!(what, "unknown key");
    assert_eq!(loc, "<root>");
}

#[test]
fn validation_empty_rules() {
    let (what, _) = parse_err("policy: t\nrules: []\n");
    assert_eq!(what, "no rules");
}

#[test]
fn validation_bad_yaml_reports_position() {
    let e = parse_policy("policy: t\nrules:\n  - id: [\n").expect_err("invalid YAML");
    assert_eq!(e.what, "invalid YAML");
    assert!(e.location.starts_with("line "));
}

#[test]
fn validation_exists_needs_boolean() {
    let (what, _) = parse_err(
        r#"
policy: t
rules:
  - id: r
    when: {x: {exists: yes_please}}
    then: {decision: approved}
"#,
    );
    assert_eq!(what, "invalid exists value");
}

#[test]
fn error_display_has_what_where_why_fix() {
    let e = parse_policy("policy: t\nrules: []\n").unwrap_err();
    let text = e.to_string();
    assert!(text.starts_with("error: no rules at rules"));
    assert!(text.contains("\n  why: "));
    assert!(text.contains("\n  fix: "));
}

#[test]
fn semantic_errors_give_the_line_of_the_problem() {
    let policy = "policy: t\nrules:\n  - id: a\n    when:\n      amount:\n        greater_than: 5\n    then:\n      decision: approved\n";
    let e = parse_policy(policy).expect_err("unknown operator");
    assert_eq!(e.what, "unknown operator");
    assert_eq!(e.location, "rules[0].when.amount.greater_than");
    assert_eq!(e.line, Some(6));
    assert!(
        e.to_string()
            .contains("rules[0].when.amount.greater_than (line 6)"),
        "{e}"
    );
}
