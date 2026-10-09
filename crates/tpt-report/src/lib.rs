//! Common report model and reporters.
//!
//! Reporters turn an [`Evaluation`] into output text. The JSON reporter is
//! the machine format and must stay stable. The terminal reporter is for
//! people. The HTML reporters are for data validation runs (see [`html`]) and
//! for one policy decision (see [`evaluation`]). Both are for filing and review.

pub mod evaluation;
pub mod html;

use tpt_policy_core::Evaluation;

pub use evaluation::evaluation_html;
pub use html::{
    data_report_html, document_report_html, DataReport, DocumentReport, DocumentRow, ReportRow,
};

/// Render an evaluation as pretty-printed JSON, the format `check` and `run` print.
pub fn json(evaluation: &Evaluation) -> String {
    serde_json::to_string_pretty(evaluation).expect("evaluation is always serialisable")
}

/// Render an evaluation as readable text. With `explain`, also list every
/// rule explanation and every failed rule with its first failed check.
pub fn terminal(evaluation: &Evaluation, explain: bool) -> String {
    let mut lines = vec![
        format!("decision: {}", evaluation.decision),
        format!("policy: {}", evaluation.policy.id()),
    ];
    if !evaluation.approvers.is_empty() {
        lines.push(format!("approvers: {}", evaluation.approvers.join(", ")));
    }
    if !evaluation.requirements.is_empty() {
        lines.push(format!(
            "requirements: {}",
            evaluation.requirements.join(", ")
        ));
    }
    if evaluation.matched_rules.is_empty() {
        lines.push("matched rules: none (default decision applied)".to_string());
    } else {
        lines.push(format!(
            "matched rules: {}",
            evaluation.matched_rules.join(", ")
        ));
    }
    for warning in &evaluation.warnings {
        lines.push(format!("warning: {warning}"));
    }
    if explain {
        lines.push(String::new());
        lines.push("explanations:".to_string());
        for e in &evaluation.explanations {
            lines.push(format!("  {} ({}): {}", e.rule, e.decision, e.message));
        }
        lines.push("failed rules:".to_string());
        for f in &evaluation.failed_rules {
            lines.push(format!("  {}: {}", f.rule, f.reason));
        }
    }
    lines.push(String::new());
    lines.join("\n")
}
