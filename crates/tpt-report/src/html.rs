//! Self-contained HTML report for a data validation run.
//!
//! The page has no scripts, no external stylesheets and no fonts, so it can be
//! opened offline and attached to an email. Every value from the input is
//! escaped before it is written.

use std::fmt::Write as _;

/// One invalid record, as shown in the report.
#[derive(Debug, Clone)]
pub struct ReportRow {
    pub number: u64,
    /// `(field, reason)` pairs.
    pub errors: Vec<(String, String)>,
}

/// Everything the data report shows. Plain data, so this crate does not depend
/// on the validator.
#[derive(Debug, Clone, Default)]
pub struct DataReport {
    pub schema_name: String,
    pub schema_version: String,
    pub policy: Option<(String, String)>,
    pub input_file: String,
    pub input_format: String,
    pub input_sha256: String,
    pub processed: u64,
    pub valid: u64,
    pub invalid: u64,
    /// `(field, count)`, most frequent first.
    pub errors_by_field: Vec<(String, u64)>,
    /// `(decision, count)` for valid records, when a policy was given.
    pub decisions: Vec<(String, u64)>,
    /// The first invalid records. See `rows_omitted`.
    pub rows: Vec<ReportRow>,
    /// How many invalid records are not listed in `rows`.
    pub rows_omitted: u64,
}

/// Render the report as one HTML document.
pub fn data_report_html(report: &DataReport) -> String {
    let mut out = String::new();
    let _ = write!(
        out,
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>Data validation: {}</title>\n<style>{CSS}</style>\n</head>\n<body>\n\
         <main>\n<h1>Data validation report</h1>\n",
        escape(&report.schema_name)
    );

    let _ = writeln!(
        out,
        "<section class=\"meta\"><dl>\
         <dt>Input</dt><dd>{} ({})</dd>\
         <dt>Schema</dt><dd>{} {}</dd>",
        escape(&report.input_file),
        escape(&report.input_format),
        escape(&report.schema_name),
        escape(&report.schema_version),
    );
    if let Some((name, version)) = &report.policy {
        let _ = writeln!(
            out,
            "<dt>Policy</dt><dd>{} {}</dd>",
            escape(name),
            escape(version)
        );
    }
    let _ = writeln!(
        out,
        "<dt>Input SHA-256</dt><dd><code>{}</code></dd></dl></section>",
        escape(&report.input_sha256)
    );

    let _ = writeln!(
        out,
        "<section class=\"counts\">\
         <div class=\"tile\"><span class=\"n\">{}</span><span class=\"l\">processed</span></div>\
         <div class=\"tile ok\"><span class=\"n\">{}</span><span class=\"l\">valid</span></div>\
         <div class=\"tile bad\"><span class=\"n\">{}</span><span class=\"l\">invalid</span></div>\
         </section>",
        group(report.processed),
        group(report.valid),
        group(report.invalid),
    );

    if !report.errors_by_field.is_empty() {
        out.push_str("<section>\n<h2>Errors by field</h2>\n<table>\n<tr><th>Field</th><th>Errors</th></tr>\n");
        for (field, count) in &report.errors_by_field {
            let _ = writeln!(
                out,
                "<tr><td><code>{}</code></td><td>{}</td></tr>",
                escape(field),
                group(*count)
            );
        }
        out.push_str("</table>\n</section>\n");
    }

    if !report.decisions.is_empty() {
        out.push_str("<section>\n<h2>Policy decisions for valid records</h2>\n<table>\n<tr><th>Decision</th><th>Records</th></tr>\n");
        for (decision, count) in &report.decisions {
            let _ = writeln!(
                out,
                "<tr><td>{}</td><td>{}</td></tr>",
                escape(decision),
                group(*count)
            );
        }
        out.push_str("</table>\n</section>\n");
    }

    if !report.rows.is_empty() {
        out.push_str("<section>\n<h2>Invalid records</h2>\n");
        for row in &report.rows {
            let _ = writeln!(out, "<article><h3>Row {}</h3><ul>", row.number);
            for (field, reason) in &row.errors {
                let _ = writeln!(
                    out,
                    "<li><code>{}</code>: {}</li>",
                    escape(field),
                    escape(reason)
                );
            }
            out.push_str("</ul></article>\n");
        }
        if report.rows_omitted > 0 {
            let _ = writeln!(
                out,
                "<p class=\"note\">{} more invalid records are listed in errors.txt and invalid.jsonl.</p>",
                group(report.rows_omitted)
            );
        }
        out.push_str("</section>\n");
    }

    out.push_str("</main>\n</body>\n</html>\n");
    out
}

/// One document in the document report.
#[derive(Debug, Clone, Default)]
pub struct DocumentRow {
    pub file: String,
    pub format: String,
    /// `PASS`, `REVIEW` or `FAIL`.
    pub verdict: String,
    /// The policy decision, with approvers, when a policy was evaluated.
    pub decision: Option<String>,
    /// `(field, reason)` pairs from the schema check.
    pub schema_errors: Vec<(String, String)>,
    /// `(rule, decision, message)` triples from the policy.
    pub explanations: Vec<(String, String, String)>,
    pub warnings: Vec<String>,
}

/// Everything the document report shows.
#[derive(Debug, Clone, Default)]
pub struct DocumentReport {
    pub schema_name: String,
    pub schema_version: String,
    pub policy: Option<(String, String)>,
    pub tool_version: String,
    pub documents: Vec<DocumentRow>,
}

/// Render the document report as one HTML document.
pub fn document_report_html(report: &DocumentReport) -> String {
    let count = |verdict: &str| {
        report
            .documents
            .iter()
            .filter(|d| d.verdict == verdict)
            .count() as u64
    };
    let mut out = String::new();
    let _ = write!(
        out,
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>Document validation: {}</title>\n<style>{CSS}</style>\n</head>\n<body>\n\
         <main>\n<h1>Document validation report</h1>\n",
        escape(&report.schema_name)
    );

    let _ = writeln!(
        out,
        "<section class=\"meta\"><dl>\
         <dt>Schema</dt><dd>{} {}</dd>",
        escape(&report.schema_name),
        escape(&report.schema_version),
    );
    if let Some((name, version)) = &report.policy {
        let _ = writeln!(
            out,
            "<dt>Policy</dt><dd>{} {}</dd>",
            escape(name),
            escape(version)
        );
    }
    let _ = writeln!(
        out,
        "<dt>Tool</dt><dd>tpt-document {}</dd></dl></section>",
        escape(&report.tool_version)
    );

    let _ = writeln!(
        out,
        "<section class=\"counts four\">\
         <div class=\"tile\"><span class=\"n\">{}</span><span class=\"l\">documents</span></div>\
         <div class=\"tile ok\"><span class=\"n\">{}</span><span class=\"l\">pass</span></div>\
         <div class=\"tile warn\"><span class=\"n\">{}</span><span class=\"l\">review</span></div>\
         <div class=\"tile bad\"><span class=\"n\">{}</span><span class=\"l\">fail</span></div>\
         </section>",
        group(report.documents.len() as u64),
        group(count("PASS")),
        group(count("REVIEW")),
        group(count("FAIL")),
    );

    out.push_str("<section>\n<h2>Documents</h2>\n<table>\n<tr><th>Document</th><th>Verdict</th><th>Decision</th></tr>\n");
    for doc in &report.documents {
        let _ = writeln!(
            out,
            "<tr><td><code>{}</code></td><td><span class=\"badge {}\">{}</span></td><td>{}</td></tr>",
            escape(&doc.file),
            verdict_class(&doc.verdict),
            escape(&doc.verdict),
            escape(doc.decision.as_deref().unwrap_or("not evaluated")),
        );
    }
    out.push_str("</table>\n</section>\n");

    out.push_str("<section>\n<h2>Details</h2>\n");
    for doc in &report.documents {
        let _ = writeln!(
            out,
            "<article><h3><code>{}</code> <span class=\"badge {}\">{}</span></h3>\
             <p class=\"note\">Format: {}</p>",
            escape(&doc.file),
            verdict_class(&doc.verdict),
            escape(&doc.verdict),
            escape(&doc.format),
        );
        if !doc.schema_errors.is_empty() {
            out.push_str("<h4>Schema errors</h4><ul>\n");
            for (field, reason) in &doc.schema_errors {
                let _ = writeln!(
                    out,
                    "<li><code>{}</code>: {}</li>",
                    escape(field),
                    escape(reason)
                );
            }
            out.push_str("</ul>\n");
        }
        if !doc.explanations.is_empty() {
            out.push_str("<h4>Policy rules</h4><ul>\n");
            for (rule, decision, message) in &doc.explanations {
                let _ = writeln!(
                    out,
                    "<li><code>{}</code> ({}): {}</li>",
                    escape(rule),
                    escape(decision),
                    escape(message)
                );
            }
            out.push_str("</ul>\n");
        }
        if !doc.warnings.is_empty() {
            out.push_str("<h4>Warnings</h4><ul>\n");
            for warning in &doc.warnings {
                let _ = writeln!(out, "<li>{}</li>", escape(warning));
            }
            out.push_str("</ul>\n");
        }
        if doc.schema_errors.is_empty() && doc.explanations.is_empty() && doc.warnings.is_empty() {
            out.push_str("<p class=\"note\">No problems found.</p>\n");
        }
        out.push_str("</article>\n");
    }
    out.push_str("</section>\n");

    out.push_str("</main>\n</body>\n</html>\n");
    out
}

/// CSS class for a verdict badge.
fn verdict_class(verdict: &str) -> &'static str {
    match verdict {
        "PASS" => "ok",
        "REVIEW" => "warn",
        "FAIL" => "bad",
        _ => "",
    }
}

/// Escape text for an HTML element or a double-quoted attribute.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            other => out.push(other),
        }
    }
    out
}

/// Thousands separators, so 1247 reads as 1,247.
fn group(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

const CSS: &str = "\
:root{--fg:#1d2330;--muted:#5b6475;--bg:#f7f8fa;--card:#ffffff;--line:#d9dde5;\
--ok:#1e7a46;--ok-bg:#e7f5ec;--warn:#8a5a00;--warn-bg:#fff4dc;--bad:#a12626;--bad-bg:#fbeaea;--code:#eef1f5}\
@media (prefers-color-scheme: dark){:root:not([data-theme=\"light\"]){\
--fg:#e8ebf0;--muted:#a3acbb;--bg:#15181e;--card:#1e222b;--line:#2f3542;\
--ok:#6fd38f;--ok-bg:#1d3327;--bad:#ff8a8a;--bad-bg:#3a2222;--warn:#f5c063;--warn-bg:#3a2f18;--code:#2a2f3a}}\
:root[data-theme=\"dark\"]{--fg:#e8ebf0;--muted:#a3acbb;--bg:#15181e;--card:#1e222b;\
--line:#2f3542;--ok:#6fd38f;--warn:#f5c063;--warn-bg:#3a2f18;--ok-bg:#1d3327;--bad:#ff8a8a;--bad-bg:#3a2222;--code:#2a2f3a}\
body{margin:0;background:var(--bg);color:var(--fg);font:16px/1.5 system-ui,-apple-system,\
Segoe UI,Roboto,sans-serif}\
main{max-width:52rem;margin:0 auto;padding:1.5rem 1rem}\
h1{font-size:1.6rem;margin:.2rem 0 1rem}h2{font-size:1.15rem;margin:1.6rem 0 .6rem}\
h3{font-size:1rem;margin:.2rem 0}\
section{margin-bottom:1rem}\
.meta{background:var(--card);border:1px solid var(--line);border-radius:8px;padding:1rem}\
dl{display:grid;grid-template-columns:max-content 1fr;gap:.35rem 1rem;margin:0}\
dt{color:var(--muted)}dd{margin:0;overflow-wrap:anywhere}\
.counts{display:grid;grid-template-columns:repeat(3,1fr);gap:.75rem}\
.tile{background:var(--card);border:1px solid var(--line);border-radius:8px;padding:.9rem;\
display:flex;flex-direction:column}\
.tile .n{font-size:1.6rem;font-weight:600}.tile .l{color:var(--muted)}\
.tile.ok{background:var(--ok-bg)}.tile.ok .n{color:var(--ok)}\
.tile.bad{background:var(--bad-bg)}.tile.bad .n{color:var(--bad)}\
table{width:100%;border-collapse:collapse;background:var(--card);border:1px solid var(--line)}\
th,td{text-align:left;padding:.45rem .6rem;border-bottom:1px solid var(--line)}\
th{color:var(--muted);font-weight:600}\
code{background:var(--code);padding:.05rem .3rem;border-radius:4px;overflow-wrap:anywhere}\
article{background:var(--card);border:1px solid var(--line);border-radius:8px;\
padding:.6rem .9rem;margin-bottom:.5rem}\
ul{margin:.3rem 0 0;padding-left:1.2rem}\
.note{color:var(--muted)}\
.tile.warn{background:var(--warn-bg)}.tile.warn .n{color:var(--warn)}\
.counts.four{grid-template-columns:repeat(4,1fr)}\
.badge{display:inline-block;padding:.05rem .5rem;border-radius:999px;font-size:.85rem;\
font-weight:600;background:var(--code)}\
.badge.ok{background:var(--ok-bg);color:var(--ok)}.badge.warn{background:var(--warn-bg);color:var(--warn)}\
.badge.bad{background:var(--bad-bg);color:var(--bad)}\
h4{font-size:.95rem;margin:.7rem 0 .2rem}\
@media (max-width:480px){.counts{grid-template-columns:1fr}.counts.four{grid-template-columns:repeat(2,1fr)}dl{grid-template-columns:1fr}dt{margin-top:.4rem}}\
";

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> DataReport {
        DataReport {
            schema_name: "customer".into(),
            schema_version: "1.0.0".into(),
            input_file: "a<b>.csv".into(),
            input_format: "csv".into(),
            input_sha256: "abc".into(),
            processed: 3,
            valid: 1,
            invalid: 2,
            errors_by_field: vec![("email".into(), 2)],
            rows: vec![ReportRow {
                number: 2,
                errors: vec![("email".into(), "'<script>' is not valid".into())],
            }],
            rows_omitted: 1,
            ..DataReport::default()
        }
    }

    #[test]
    fn escapes_input_text() {
        assert_eq!(
            escape("<a href=\"x\">&'"),
            "&lt;a href=&quot;x&quot;&gt;&amp;&#39;"
        );
    }

    #[test]
    fn page_escapes_values_and_has_no_scripts_or_external_links() {
        let html = data_report_html(&sample());
        assert!(html.contains("a&lt;b&gt;.csv"), "file name escaped");
        assert!(
            html.contains("&#39;&lt;script&gt;&#39; is not valid"),
            "reason escaped"
        );
        assert!(!html.contains("<script"), "no script tags");
        assert!(
            !html.contains("http://") && !html.contains("https://"),
            "no external links"
        );
        assert!(
            html.contains("1 more invalid records"),
            "omitted count shown"
        );
    }

    fn document_sample() -> DocumentReport {
        DocumentReport {
            schema_name: "invoice".into(),
            schema_version: "1.0.0".into(),
            policy: Some(("invoice-approval".into(), "1.0.0".into())),
            tool_version: "2026.1.0".into(),
            documents: vec![
                DocumentRow {
                    file: "ok.xml".into(),
                    format: "xml".into(),
                    verdict: "PASS".into(),
                    decision: Some("approved".into()),
                    ..DocumentRow::default()
                },
                DocumentRow {
                    file: "<bad>.json".into(),
                    format: "json".into(),
                    verdict: "FAIL".into(),
                    schema_errors: vec![("currency".into(), "'XYZ' is not valid".into())],
                    ..DocumentRow::default()
                },
            ],
        }
    }

    #[test]
    fn document_page_counts_verdicts_and_escapes_values() {
        let html = document_report_html(&document_sample());
        assert!(html.contains("&lt;bad&gt;.json"), "file name escaped");
        assert!(
            html.contains("&#39;XYZ&#39; is not valid"),
            "reason escaped"
        );
        assert!(!html.contains("<script"), "no script tags");
        assert!(
            !html.contains("http://") && !html.contains("https://"),
            "no external links"
        );
        assert!(
            html.contains("not evaluated"),
            "document without a policy decision"
        );
        assert!(html.contains("<span class=\"n\">2</span><span class=\"l\">documents</span>"));
        assert!(html.contains("<span class=\"n\">1</span><span class=\"l\">pass</span>"));
        assert!(html.contains("<span class=\"n\">0</span><span class=\"l\">review</span>"));
        assert!(html.contains("<span class=\"n\">1</span><span class=\"l\">fail</span>"));
    }
}
