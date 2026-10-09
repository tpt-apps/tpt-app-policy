//! Self-contained HTML page for one policy decision.
//!
//! Like the data and document reports, the page has no scripts, no external
//! links and no fonts, and every value is escaped. It is for a person to read
//! and file. The JSON reporter remains the record that a system keeps.

use std::fmt::Write as _;

use tpt_policy_core::{Decision, Evaluation};

use crate::html::{escape, CSS};

/// Render one evaluation as an HTML document.
pub fn evaluation_html(evaluation: &Evaluation) -> String {
    let decision = evaluation.decision;
    let mut out = String::new();
    let _ = write!(
        out,
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>Decision: {}</title>\n<style>{CSS}</style>\n</head>\n<body>\n\
         <main>\n<h1>Policy decision</h1>\n",
        escape(decision.as_str())
    );

    let _ = writeln!(
        out,
        "<section class=\"meta\"><dl>\
         <dt><abbr title=\"The policy file and its version that made this decision\">Policy</abbr></dt><dd>{}</dd>\
         <dt><abbr title=\"A fingerprint of the input. The same input always gives the same value. The input itself is not stored in this report\">Input SHA-256</abbr></dt><dd><code>{}</code></dd>\
         <dt><abbr title=\"The version of the rules engine that ran the policy\">Engine</abbr></dt><dd>tpt-policy {}</dd>\
         </dl></section>",
        escape(&evaluation.policy.id()),
        escape(&evaluation.input_sha256),
        escape(evaluation.engine_version),
    );

    let _ = writeln!(
        out,
        "<section class=\"counts four\">\
         <div class=\"tile {}\"><span class=\"n\">{}</span><span class=\"l\"><abbr title=\"The outcome. approved: no action needed. review: a person should look. approval_required: someone must approve. rejected: do not proceed\">decision</abbr></span></div>\
         <div class=\"tile\"><span class=\"n\">{}</span><span class=\"l\"><abbr title=\"Rules whose conditions the input met\">matched rules</abbr></span></div>\
         <div class=\"tile\"><span class=\"n\">{}</span><span class=\"l\"><abbr title=\"The people or roles who must approve, from the matched rules\">approvers</abbr></span></div>\
         <div class=\"tile\"><span class=\"n\">{}</span><span class=\"l\"><abbr title=\"Rules whose conditions the input did not meet. Listed below with the first check that failed\">failed rules</abbr></span></div>\
         </section>",
        tile_class(decision),
        escape(decision.as_str()),
        evaluation.matched_rules.len(),
        evaluation.approvers.len(),
        evaluation.failed_rules.len(),
    );

    if !evaluation.approvers.is_empty() || !evaluation.requirements.is_empty() {
        out.push_str("<h2>Who must act</h2>\n<section class=\"meta\">\n");
        list(&mut out, "Approvers", &evaluation.approvers);
        list(&mut out, "Requirements", &evaluation.requirements);
        out.push_str("</section>\n");
    }
    if !evaluation.warnings.is_empty() {
        out.push_str("<h2>Warnings</h2>\n<section class=\"meta\">\n");
        list(&mut out, "Warnings", &evaluation.warnings);
        out.push_str("</section>\n");
    }

    out.push_str("<h2>Matched rules</h2>\n");
    if evaluation.explanations.is_empty() {
        out.push_str(
            "<p class=\"note\">No rule matched, so the policy's default decision applies.</p>\n",
        );
    } else {
        out.push_str("<table>\n<thead><tr><th><abbr title=\"The rule's id in the policy file\">Rule</abbr></th><th><abbr title=\"The decision this rule gives when it matches\">Decision</abbr></th><th><abbr title=\"What the rule says, in the policy's own words\">Why</abbr></th></tr></thead>\n<tbody>\n");
        for explanation in &evaluation.explanations {
            let _ = writeln!(
                out,
                "<tr><td><code>{}</code></td><td><span class=\"badge {}\">{}</span></td><td>{}</td></tr>",
                escape(&explanation.rule),
                badge_class(explanation.decision),
                escape(explanation.decision.as_str()),
                escape(&explanation.message),
            );
        }
        out.push_str("</tbody>\n</table>\n");
    }

    if !evaluation.failed_rules.is_empty() {
        out.push_str("<h2>Rules that did not match</h2>\n<table>\n<thead><tr><th><abbr title=\"The rule's id in the policy file\">Rule</abbr></th><th><abbr title=\"The first condition in the rule that the input did not meet. Fix the input or the rule to change the outcome\">First failed check</abbr></th></tr></thead>\n<tbody>\n");
        for failed in &evaluation.failed_rules {
            let _ = writeln!(
                out,
                "<tr><td><code>{}</code></td><td>{}</td></tr>",
                escape(&failed.rule),
                escape(&failed.reason),
            );
        }
        out.push_str("</tbody>\n</table>\n");
    }

    out.push_str("</main>\n</body>\n</html>\n");
    out
}

fn list(out: &mut String, label: &str, items: &[String]) {
    if items.is_empty() {
        return;
    }
    let _ = write!(out, "<h3>{}</h3>\n<ul>", escape(label));
    for item in items {
        let _ = write!(out, "<li>{}</li>", escape(item));
    }
    out.push_str("</ul>\n");
}

/// CSS class for the decision tile.
fn tile_class(decision: Decision) -> &'static str {
    match decision {
        Decision::Approved => "ok",
        Decision::Review | Decision::ApprovalRequired => "warn",
        Decision::Rejected => "bad",
    }
}

/// CSS class for a decision badge in the matched rules table.
fn badge_class(decision: Decision) -> &'static str {
    match decision {
        Decision::Approved => "ok",
        Decision::Review | Decision::ApprovalRequired => "warn",
        Decision::Rejected => "bad",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tpt_policy_core::{evaluate, parse_policy};

    const POLICY: &str = "\
policy: <expenses>
rules:
  - id: big
    description: Large <expense>
    when:
      amount:
        gt: 1000
    then:
      decision: approval_required
      approver: manager
      requirement: receipt & reason
  - id: small
    when:
      amount:
        lte: 1000
    then:
      decision: approved
";

    #[test]
    fn page_shows_the_decision_and_escapes_values() {
        let policy = parse_policy(POLICY).unwrap();
        let evaluation = evaluate(&policy, &serde_json::json!({"amount": 2500}));
        let html = evaluation_html(&evaluation);
        assert!(html.contains("approval_required"));
        assert!(
            html.contains("&lt;expenses&gt;@unversioned"),
            "policy name escaped"
        );
        assert!(html.contains("receipt &amp; reason"), "requirement escaped");
        assert!(html.contains("<code>big</code>"), "matched rule shown");
        assert!(!html.contains("<script"));
        assert!(!html.contains("http://") && !html.contains("https://"));
    }

    #[test]
    fn page_says_when_no_rule_matched() {
        let policy = parse_policy(POLICY).unwrap();
        let evaluation = evaluate(&policy, &serde_json::json!({"other": 1}));
        let html = evaluation_html(&evaluation);
        assert!(html.contains("No rule matched"));
    }
}
