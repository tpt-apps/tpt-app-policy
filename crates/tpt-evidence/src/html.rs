//! Self-contained HTML report for an evidence run. No scripts, no external
//! links. Every value is escaped.

use std::fmt::Write as _;

use tpt_report::html::escape;

use crate::{Report, DISCLAIMER};

pub fn evidence_report_html(report: &Report) -> String {
    let mut out = String::new();
    let _ = write!(
        out,
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>Evidence report: {}</title>\n<style>{CSS}</style>\n</head>\n<body>\n<main>\n\
         <h1>Evidence report</h1>\n<p class=\"disclaimer\">{}</p>\n",
        escape(&report.name),
        escape(DISCLAIMER)
    );

    let _ = writeln!(
        out,
        "<section class=\"meta\"><dl>\
         <dt>Evidence set</dt><dd>{}</dd>\
         <dt>Framework</dt><dd>{}</dd>\
         <dt>As of</dt><dd>{}</dd>\
         <dt>Result</dt><dd><span class=\"badge {}\">{}</span></dd>\
         <dt>Manifest SHA-256</dt><dd><code>{}</code></dd>\
         <dt>Tool</dt><dd>tpt-evidence {}</dd></dl></section>",
        escape(&report.name),
        escape(&report.framework),
        escape(&report.as_of),
        if report.status == "complete" {
            "ok"
        } else {
            "bad"
        },
        escape(&report.status),
        escape(&report.manifest.manifest_sha256),
        escape(report.tool_version),
    );

    let s = &report.summary;
    let _ = writeln!(
        out,
        "<section class=\"counts\">\
         <div class=\"tile ok\"><span class=\"n\">{}/{}</span><span class=\"l\">controls covered</span></div>\
         <div class=\"tile\"><span class=\"n\">{}</span><span class=\"l\">evidence items</span></div>\
         <div class=\"tile bad\"><span class=\"n\">{}</span><span class=\"l\">items with errors</span></div>\
         <div class=\"tile warn\"><span class=\"n\">{}</span><span class=\"l\">items with warnings</span></div>\
         </section>",
        s.controls_covered, s.controls, s.items, s.items_with_errors, s.items_with_warnings
    );

    out.push_str("<section>\n<h2>Controls</h2>\n<table>\n<tr><th>Control</th><th>Status</th><th>Usable evidence</th><th>Not usable</th><th>Needed</th></tr>\n");
    for control in &report.controls {
        let _ = writeln!(
            out,
            "<tr><td><code>{}</code> {}</td><td><span class=\"badge {}\">{}</span></td><td>{}</td><td>{}</td><td>{}</td></tr>",
            escape(&control.id),
            escape(&control.title),
            control_class(&control.status),
            escape(&control.status),
            escape(&control.usable.join(", ")),
            escape(&control.unusable.join(", ")),
            control.min_items
        );
    }
    out.push_str("</table>\n</section>\n");

    out.push_str("<section>\n<h2>Evidence</h2>\n");
    for item in &report.items {
        let _ = writeln!(
            out,
            "<article><h3><code>{}</code> <span class=\"badge {}\">{}</span></h3>\
             <p class=\"note\">{} &middot; {} &middot; collected {} &middot; {} bytes{}</p>\
             <p class=\"note\">SHA-256 <code>{}</code></p>",
            escape(&item.id),
            item_class(&item.status),
            escape(&item.status),
            escape(&item.file),
            escape(&item.kind),
            escape(&item.collected),
            item.bytes,
            item.detail
                .as_ref()
                .map(|d| format!(" &middot; {}", escape(d)))
                .unwrap_or_default(),
            escape(&item.sha256),
        );
        if !item.controls.is_empty() {
            let _ = writeln!(
                out,
                "<p class=\"note\">Supports: {}</p>",
                escape(&item.controls.join(", "))
            );
        }
        if let Some(owner) = &item.owner {
            let _ = writeln!(out, "<p class=\"note\">Owner: {}</p>", escape(owner));
        }
        if !item.findings.is_empty() {
            out.push_str("<ul>\n");
            for finding in &item.findings {
                let _ = writeln!(
                    out,
                    "<li><span class=\"badge {}\">{}</span> {}</li>",
                    if finding.level == "error" {
                        "bad"
                    } else {
                        "warn"
                    },
                    escape(&finding.level),
                    escape(&finding.message)
                );
            }
            out.push_str("</ul>\n");
        }
        out.push_str("</article>\n");
    }
    out.push_str("</section>\n");

    if !report.index_findings.is_empty() {
        out.push_str("<section>\n<h2>Notes about the set</h2>\n<ul>\n");
        for finding in &report.index_findings {
            let _ = writeln!(out, "<li>{}</li>", escape(&finding.message));
        }
        out.push_str("</ul>\n</section>\n");
    }

    out.push_str("<section>\n<h2>Manifest</h2>\n<table>\n<tr><th>File</th><th>SHA-256</th><th>Bytes</th></tr>\n");
    for entry in &report.manifest.entries {
        let _ = writeln!(
            out,
            "<tr><td><code>{}</code></td><td><code>{}</code></td><td>{}</td></tr>",
            escape(&entry.path),
            escape(&entry.sha256),
            entry.bytes
        );
    }
    out.push_str("</table>\n</section>\n");

    out.push_str("</main>\n</body>\n</html>\n");
    out
}

fn control_class(status: &str) -> &'static str {
    match status {
        "covered" => "ok",
        "stale" => "warn",
        _ => "bad",
    }
}

fn item_class(status: &str) -> &'static str {
    match status {
        "ok" => "ok",
        "warning" => "warn",
        _ => "bad",
    }
}

const CSS: &str = "\
:root{--fg:#1d2330;--muted:#5b6475;--bg:#f7f8fa;--card:#ffffff;--line:#d9dde5;\
--ok:#1e7a46;--ok-bg:#e7f5ec;--warn:#8a5a00;--warn-bg:#fff4dc;--bad:#a12626;--bad-bg:#fbeaea;--code:#eef1f5}\
@media (prefers-color-scheme: dark){:root:not([data-theme=\"light\"]){\
--fg:#e8ebf0;--muted:#a3acbb;--bg:#15181e;--card:#1e222b;--line:#2f3542;\
--ok:#6fd38f;--ok-bg:#1d3327;--warn:#f5c063;--warn-bg:#3a2f18;--bad:#ff8a8a;--bad-bg:#3a2222;--code:#2a2f3a}}\
:root[data-theme=\"dark\"]{--fg:#e8ebf0;--muted:#a3acbb;--bg:#15181e;--card:#1e222b;--line:#2f3542;\
--ok:#6fd38f;--ok-bg:#1d3327;--warn:#f5c063;--warn-bg:#3a2f18;--bad:#ff8a8a;--bad-bg:#3a2222;--code:#2a2f3a}\
body{margin:0;background:var(--bg);color:var(--fg);font:16px/1.5 system-ui,-apple-system,Segoe UI,Roboto,sans-serif}\
main{max-width:52rem;margin:0 auto;padding:1.5rem 1rem}\
h1{font-size:1.6rem;margin:.2rem 0 .5rem}h2{font-size:1.15rem;margin:1.6rem 0 .6rem}h3{font-size:1rem;margin:.2rem 0}\
.disclaimer{background:var(--warn-bg);color:var(--warn);border:1px solid var(--line);border-radius:8px;padding:.7rem .9rem;margin:0 0 1rem}\
section{margin-bottom:1rem}\
.meta{background:var(--card);border:1px solid var(--line);border-radius:8px;padding:1rem}\
dl{display:grid;grid-template-columns:max-content 1fr;gap:.35rem 1rem;margin:0}dt{color:var(--muted)}dd{margin:0;overflow-wrap:anywhere}\
.counts{display:grid;grid-template-columns:repeat(4,1fr);gap:.75rem}\
.tile{background:var(--card);border:1px solid var(--line);border-radius:8px;padding:.9rem;display:flex;flex-direction:column}\
.tile .n{font-size:1.5rem;font-weight:600}.tile .l{color:var(--muted)}\
.tile.ok{background:var(--ok-bg)}.tile.ok .n{color:var(--ok)}.tile.warn{background:var(--warn-bg)}.tile.warn .n{color:var(--warn)}\
.tile.bad{background:var(--bad-bg)}.tile.bad .n{color:var(--bad)}\
table{width:100%;border-collapse:collapse;background:var(--card);border:1px solid var(--line)}\
th,td{text-align:left;padding:.45rem .6rem;border-bottom:1px solid var(--line);vertical-align:top}th{color:var(--muted);font-weight:600}\
code{background:var(--code);padding:.05rem .3rem;border-radius:4px;overflow-wrap:anywhere}\
article{background:var(--card);border:1px solid var(--line);border-radius:8px;padding:.6rem .9rem;margin-bottom:.5rem}\
.note{color:var(--muted);margin:.2rem 0}ul{margin:.3rem 0 0;padding-left:1.2rem}\
.badge{display:inline-block;padding:.05rem .5rem;border-radius:999px;font-size:.85rem;font-weight:600;background:var(--code)}\
.badge.ok{background:var(--ok-bg);color:var(--ok)}.badge.warn{background:var(--warn-bg);color:var(--warn)}.badge.bad{background:var(--bad-bg);color:var(--bad)}\
@media (max-width:480px){.counts{grid-template-columns:repeat(2,1fr)}dl{grid-template-columns:1fr}dt{margin-top:.4rem}}\
";
