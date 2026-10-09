//! Embedding the policy engine in a Rust program.
//!
//! Run with `cargo run -p tpt-policy-core --example embed`. `cargo test` also
//! builds this file, so the API shown here stays correct.

use serde_json::json;
use tpt_policy_core::{evaluate, parse_policy, run_tests, Decision};

const POLICY: &str = "\
policy: expenses
version: \"1.0.0\"
rules:
  - id: large
    when:
      amount:
        gt: 1000
    then:
      decision: approval_required
      approver: manager
  - id: small
    when:
      amount:
        lte: 1000
    then:
      decision: approved
tests:
  - name: small expense is approved
    input: {amount: 40}
    expect:
      decision: approved
      matched_rules: [small]
";

fn main() {
    // 1. Parse once. Errors say what, where, why and how to fix.
    let policy = match parse_policy(POLICY) {
        Ok(policy) => policy,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };

    // 2. Run the policy's own tests. Useful in a host's test suite.
    for outcome in run_tests(&policy) {
        println!("{}: {}", outcome.name, outcome.message);
    }

    // 3. Evaluate an input. The same input always gives the same result.
    let evaluation = evaluate(&policy, &json!({"amount": 2500}));
    println!("decision: {}", evaluation.decision);
    println!("approvers: {:?}", evaluation.approvers);
    println!("matched rules: {:?}", evaluation.matched_rules);

    // 4. Act on the decision.
    match evaluation.decision {
        Decision::Approved => println!("post the expense"),
        Decision::Review | Decision::ApprovalRequired => println!("send for approval"),
        Decision::Rejected => println!("reject the expense"),
    }
}
