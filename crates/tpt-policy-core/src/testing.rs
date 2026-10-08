//! Runs the inline `tests:` block of a policy file (spec §36).

use crate::eval::evaluate;
use crate::model::Policy;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestOutcome {
    pub name: String,
    pub passed: bool,
    /// `ok` when passed, otherwise the differences found.
    pub message: String,
}

/// Evaluate every inline test and report what differed from expectations.
pub fn run_tests(policy: &Policy) -> Vec<TestOutcome> {
    policy
        .tests
        .iter()
        .map(|test| {
            let evaluation = evaluate(policy, &test.input);
            let mut problems = Vec::new();

            if evaluation.decision != test.expect.decision {
                problems.push(format!(
                    "expected decision {} but got {}",
                    test.expect.decision, evaluation.decision
                ));
            }
            if let Some(expected) = &test.expect.matched_rules {
                if &evaluation.matched_rules != expected {
                    problems.push(format!(
                        "expected matched rules {:?} but got {:?}",
                        expected, evaluation.matched_rules
                    ));
                }
            }

            let passed = problems.is_empty();
            TestOutcome {
                name: test.name.clone(),
                passed,
                message: if passed {
                    "ok".to_string()
                } else {
                    problems.join("; ")
                },
            }
        })
        .collect()
}
