//! Evidence processing for TPT Compliance Evidence Processor.
//!
//! The processor reads an evidence index and a controls list, checks every
//! evidence file, maps evidence to controls, and writes an inventory, a
//! manifest of hashes, and reports. It does not decide whether an organisation
//! complies with anything. Every report says so.
//!
//! Time is never read from the clock. The date to check against is given with
//! `--as-of`, so the same inputs always give the same report.

pub mod html;

use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tpt_document::{read_document, Parsed};
use tpt_policy_core::parse_policy;

/// Version of this crate, recorded in reports.
pub const EVIDENCE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Shown on every report. The processor organises evidence; it does not certify.
pub const DISCLAIMER: &str = "This report organises and checks evidence. It does not certify that any organisation complies with a framework, standard or law.";

// --- inputs -----------------------------------------------------------------

/// The evidence index: which files exist, and which controls each supports.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceIndex {
    pub name: String,
    pub items: Vec<EvidenceItem>,
}

/// One evidence file and what it is evidence for.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceItem {
    pub id: String,
    /// Path relative to the index file.
    pub file: String,
    /// `json`, `csv`, `text`, `log`, `policy` or `document`. Inferred from the extension when omitted.
    #[serde(default, rename = "type")]
    pub kind: Option<String>,
    /// Control IDs this evidence supports.
    #[serde(default)]
    pub controls: Vec<String>,
    /// The date the evidence was collected, as YYYY-MM-DD.
    pub collected: String,
    #[serde(default)]
    pub owner: Option<String>,
    /// SHA-256 recorded when the evidence was collected. Checked against the file.
    #[serde(default)]
    pub sha256: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

/// The controls to map evidence to.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Controls {
    pub framework: String,
    pub controls: Vec<Control>,
}

/// One control. Evidence must be at most `max_age_days` old, if that is set.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Control {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub max_age_days: Option<i64>,
    /// How many usable evidence items the control needs. Defaults to 1.
    #[serde(default = "one")]
    pub min_items: usize,
}

fn one() -> usize {
    1
}

pub fn parse_index(text: &str) -> Result<EvidenceIndex, String> {
    let index: EvidenceIndex = serde_yaml::from_str(text).map_err(|e| e.to_string())?;
    if index.items.is_empty() {
        return Err(
            "the index has no items; list at least one evidence file under 'items:'".into(),
        );
    }
    let mut ids = std::collections::BTreeSet::new();
    for item in &index.items {
        if !ids.insert(item.id.clone()) {
            return Err(format!(
                "evidence id '{}' is used twice; give each item its own id",
                item.id
            ));
        }
        parse_date(&item.collected)
            .map_err(|why| format!("evidence '{}': collected: {why}", item.id))?;
        if item.file.trim().is_empty() {
            return Err(format!("evidence '{}': 'file' is empty", item.id));
        }
    }
    Ok(index)
}

pub fn parse_controls(text: &str) -> Result<Controls, String> {
    let controls: Controls = serde_yaml::from_str(text).map_err(|e| e.to_string())?;
    if controls.controls.is_empty() {
        return Err("the controls file has no controls; add at least one under 'controls:'".into());
    }
    let mut ids = std::collections::BTreeSet::new();
    for control in &controls.controls {
        if !ids.insert(control.id.clone()) {
            return Err(format!("control id '{}' is used twice", control.id));
        }
        if control.min_items == 0 {
            return Err(format!(
                "control '{}': min_items must be at least 1",
                control.id
            ));
        }
    }
    Ok(controls)
}

// --- dates --------------------------------------------------------------------

/// Days since 1970-01-01 for a YYYY-MM-DD date. Rejects dates that do not exist.
pub fn parse_date(text: &str) -> Result<i64, String> {
    let parts: Vec<&str> = text.trim().split('-').collect();
    let [y, m, d] = parts.as_slice() else {
        return Err(format!("'{text}' is not a date in YYYY-MM-DD form"));
    };
    let (Ok(year), Ok(month), Ok(day)) = (y.parse::<i64>(), m.parse::<u32>(), d.parse::<u32>())
    else {
        return Err(format!("'{text}' is not a date in YYYY-MM-DD form"));
    };
    if y.len() != 4 || !(1..=12).contains(&month) || day == 0 || day > days_in_month(year, month) {
        return Err(format!("'{text}' is not a real date"));
    }
    Ok(days_from_civil(year, month, day))
}

fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 {
                29
            } else {
                28
            }
        }
    }
}

/// Howard Hinnant's algorithm: days since 1970-01-01 for a civil date.
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = (if y >= 0 { y } else { y - 399 }) / 400;
    let yoe = y - era * 400;
    let m = i64::from(month);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

// --- processing ---------------------------------------------------------------

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Finding {
    /// `error` or `warning`.
    pub level: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ItemReport {
    pub id: String,
    pub file: String,
    pub kind: String,
    pub bytes: u64,
    pub sha256: String,
    pub controls: Vec<String>,
    pub collected: String,
    pub owner: Option<String>,
    pub description: Option<String>,
    /// `ok`, `warning` or `error`.
    pub status: String,
    /// Records, lines, or nothing, depending on the kind.
    pub detail: Option<String>,
    pub findings: Vec<Finding>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ControlReport {
    pub id: String,
    pub title: String,
    pub min_items: usize,
    pub max_age_days: Option<i64>,
    /// `covered`, `stale` (evidence exists but is too old) or `gap` (none usable).
    pub status: String,
    /// Usable evidence items for this control.
    pub usable: Vec<String>,
    /// Evidence items listed for this control that cannot be used.
    pub unusable: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Summary {
    pub controls: usize,
    pub controls_covered: usize,
    pub items: usize,
    pub items_with_errors: usize,
    pub items_with_warnings: usize,
}

/// The full result of processing an index.
#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub tool: &'static str,
    pub tool_version: &'static str,
    pub disclaimer: &'static str,
    pub name: String,
    pub framework: String,
    pub as_of: String,
    /// `complete` when every control is covered and no item has an error. Otherwise `incomplete`.
    pub status: String,
    pub summary: Summary,
    pub controls: Vec<ControlReport>,
    pub items: Vec<ItemReport>,
    pub manifest: Manifest,
    /// Findings about the index as a whole, such as an item that names an unknown control.
    pub index_findings: Vec<Finding>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ManifestEntry {
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
}

/// Hashes of every evidence file. `manifest_sha256` covers the entries, so a
/// changed or removed entry shows up when the manifest is verified.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Manifest {
    pub entries: Vec<ManifestEntry>,
    pub manifest_sha256: String,
}

/// Process an evidence index. `base` is the folder the item paths are relative to.
pub fn process(
    index: &EvidenceIndex,
    controls: &Controls,
    base: &Path,
    as_of: &str,
) -> Result<Report, String> {
    let as_of_days = parse_date(as_of).map_err(|why| format!("--as-of: {why}"))?;
    let known: std::collections::BTreeSet<&str> =
        controls.controls.iter().map(|c| c.id.as_str()).collect();

    let mut index_findings = Vec::new();
    let mut items = Vec::new();
    for item in &index.items {
        let mut report = inspect(item, base);
        for control in &item.controls {
            if !known.contains(control.as_str()) {
                report.findings.push(Finding {
                    level: "warning".into(),
                    message: format!(
                        "names control '{control}', which is not in the controls file"
                    ),
                });
            }
        }
        if parse_date(&item.collected).unwrap_or(0) > as_of_days {
            report.findings.push(Finding {
                level: "warning".into(),
                message: format!(
                    "collected {} is after the as-of date {as_of}",
                    item.collected
                ),
            });
        }
        report.status = status_of(&report.findings);
        items.push(report);
    }
    for control in &controls.controls {
        let holders = index
            .items
            .iter()
            .filter(|i| i.controls.contains(&control.id))
            .count();
        if holders == 0 {
            index_findings.push(Finding {
                level: "warning".into(),
                message: format!("control '{}' has no evidence listed", control.id),
            });
        }
    }

    let mut control_reports = Vec::new();
    for control in &controls.controls {
        let mut usable = Vec::new();
        let mut unusable = Vec::new();
        let mut any_listed_but_old = false;
        for (item, report) in index.items.iter().zip(&items) {
            if !item.controls.contains(&control.id) {
                continue;
            }
            let collected = parse_date(&item.collected).unwrap_or(0);
            let age = as_of_days - collected;
            let too_old = control.max_age_days.is_some_and(|max| age > max);
            if too_old {
                any_listed_but_old = true;
            }
            if report.status == "error" || too_old {
                unusable.push(item.id.clone());
            } else {
                usable.push(item.id.clone());
            }
        }
        let status = if usable.len() >= control.min_items {
            "covered"
        } else if any_listed_but_old && usable.is_empty() {
            "stale"
        } else {
            "gap"
        };
        control_reports.push(ControlReport {
            id: control.id.clone(),
            title: control.title.clone(),
            min_items: control.min_items,
            max_age_days: control.max_age_days,
            status: status.into(),
            usable,
            unusable,
        });
    }

    let covered = control_reports
        .iter()
        .filter(|c| c.status == "covered")
        .count();
    let items_with_errors = items.iter().filter(|i| i.status == "error").count();
    let items_with_warnings = items.iter().filter(|i| i.status == "warning").count();
    let complete = covered == control_reports.len() && items_with_errors == 0;
    let manifest = build_manifest(&items);

    Ok(Report {
        tool: "tpt-evidence",
        tool_version: EVIDENCE_VERSION,
        disclaimer: DISCLAIMER,
        name: index.name.clone(),
        framework: controls.framework.clone(),
        as_of: as_of.to_string(),
        status: if complete { "complete" } else { "incomplete" }.into(),
        summary: Summary {
            controls: control_reports.len(),
            controls_covered: covered,
            items: items.len(),
            items_with_errors,
            items_with_warnings,
        },
        controls: control_reports,
        items,
        manifest,
        index_findings,
    })
}

fn status_of(findings: &[Finding]) -> String {
    if findings.iter().any(|f| f.level == "error") {
        "error".into()
    } else if findings.iter().any(|f| f.level == "warning") {
        "warning".into()
    } else {
        "ok".into()
    }
}

/// Read and check one evidence file. Problems become findings, not panics.
fn inspect(item: &EvidenceItem, base: &Path) -> ItemReport {
    let path = base.join(&item.file);
    let kind = item.kind.clone().unwrap_or_else(|| infer_kind(&item.file));
    let mut report = ItemReport {
        id: item.id.clone(),
        file: item.file.clone(),
        kind: kind.clone(),
        bytes: 0,
        sha256: String::new(),
        controls: item.controls.clone(),
        collected: item.collected.clone(),
        owner: item.owner.clone(),
        description: item.description.clone(),
        status: "ok".into(),
        detail: None,
        findings: Vec::new(),
    };
    if !matches!(
        kind.as_str(),
        "json" | "csv" | "text" | "log" | "policy" | "document"
    ) {
        report.findings.push(Finding {
            level: "error".into(),
            message: format!(
                "unknown evidence type '{kind}'; use json, csv, text, log, policy or document"
            ),
        });
        report.status = "error".into();
        return report;
    }
    if !is_safe_relative(&item.file) {
        report.findings.push(Finding {
            level: "error".into(),
            message: "the path must be relative and stay inside the index folder".into(),
        });
        report.status = "error".into();
        return report;
    }
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(e) => {
            report.findings.push(Finding {
                level: "error".into(),
                message: format!("cannot read the file: {e}"),
            });
            report.status = "error".into();
            return report;
        }
    };
    report.bytes = bytes.len() as u64;
    report.sha256 = sha256_hex(&bytes);

    if let Some(declared) = &item.sha256 {
        if !declared.eq_ignore_ascii_case(&report.sha256) {
            report.findings.push(Finding {
                level: "error".into(),
                message: "the file has changed since its hash was recorded".into(),
            });
        }
    }
    report.detail = check_contents(&kind, &path, &bytes, &mut report.findings);
    report.status = status_of(&report.findings);
    report
}

fn check_contents(
    kind: &str,
    path: &Path,
    bytes: &[u8],
    findings: &mut Vec<Finding>,
) -> Option<String> {
    let error = |findings: &mut Vec<Finding>, message: String| {
        findings.push(Finding {
            level: "error".into(),
            message,
        });
    };
    match kind {
        "json" => match serde_json::from_slice::<Value>(bytes) {
            Ok(_) => None,
            Err(e) => {
                error(findings, format!("not valid JSON: {e}"));
                None
            }
        },
        "csv" => {
            let mut reader = csv::ReaderBuilder::new()
                .has_headers(true)
                .from_reader(bytes);
            let mut records = 0usize;
            for row in reader.records() {
                match row {
                    Ok(_) => records += 1,
                    Err(e) => {
                        error(findings, format!("not valid CSV: {e}"));
                        return None;
                    }
                }
            }
            Some(format!("{records} data rows"))
        }
        "policy" => match std::str::from_utf8(bytes) {
            Ok(text) => match parse_policy(text) {
                Ok(_) => None,
                Err(e) => {
                    error(findings, format!("not a valid policy: {}", e.why));
                    None
                }
            },
            Err(_) => {
                error(findings, "not UTF-8 text".into());
                None
            }
        },
        "document" => match read_document(path) {
            Ok(Parsed::Ok { .. }) => None,
            Ok(Parsed::Malformed { message, .. }) => {
                error(findings, format!("not a valid document: {message}"));
                None
            }
            Err(e) => {
                error(findings, e.why);
                None
            }
        },
        _ => match std::str::from_utf8(bytes) {
            Ok(text) => Some(format!("{} lines", text.lines().count())),
            Err(_) => {
                error(findings, "not UTF-8 text".into());
                None
            }
        },
    }
}

fn infer_kind(file: &str) -> String {
    let ext = Path::new(file)
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "json" => "json",
        "csv" => "csv",
        "xml" => "document",
        "yaml" | "yml" => "policy",
        "log" => "log",
        _ => "text",
    }
    .to_string()
}

/// True for a path that is relative and does not go up out of its folder.
fn is_safe_relative(file: &str) -> bool {
    let path = Path::new(file);
    !path.is_absolute()
        && path
            .components()
            .all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Build the manifest from the items that could be read. Entries are sorted by path.
fn build_manifest(items: &[ItemReport]) -> Manifest {
    let mut entries: Vec<ManifestEntry> = items
        .iter()
        .filter(|i| !i.sha256.is_empty())
        .map(|i| ManifestEntry {
            path: normalise_path(&i.file),
            sha256: i.sha256.clone(),
            bytes: i.bytes,
        })
        .collect();
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    let manifest_sha256 = manifest_hash(&entries);
    Manifest {
        entries,
        manifest_sha256,
    }
}

fn manifest_hash(entries: &[ManifestEntry]) -> String {
    let canonical = serde_json::to_string(entries).expect("entries serialise");
    sha256_hex(canonical.as_bytes())
}

fn normalise_path(file: &str) -> String {
    file.replace('\\', "/")
}

/// Check a manifest against the files under `root`. Returns one line per entry
/// and whether everything matched.
pub fn verify(manifest: &Manifest, root: &Path) -> (bool, Vec<(String, String)>) {
    let mut lines = Vec::new();
    let mut ok = true;
    if manifest_hash(&manifest.entries) != manifest.manifest_sha256 {
        ok = false;
        lines.push((
            "manifest".to_string(),
            "the manifest hash does not match its entries; the manifest was edited".to_string(),
        ));
    }
    for entry in &manifest.entries {
        let Some(path) = safe_join(root, &entry.path) else {
            ok = false;
            lines.push((entry.path.clone(), "unsafe path".to_string()));
            continue;
        };
        let status = match fs::read(&path) {
            Err(_) => "missing".to_string(),
            Ok(bytes) if sha256_hex(&bytes) == entry.sha256 => "ok".to_string(),
            Ok(_) => "changed".to_string(),
        };
        if status != "ok" {
            ok = false;
        }
        lines.push((entry.path.clone(), status));
    }
    (ok, lines)
}

/// Resolve a path from a manifest against a root, refusing anything that escapes it.
pub fn safe_join(root: &Path, relative: &str) -> Option<PathBuf> {
    is_safe_relative(relative).then(|| root.join(relative))
}

/// The machine-readable report as JSON text. Keys are sorted, so output is stable.
pub fn report_json(report: &Report) -> String {
    let value: Value = serde_json::to_value(report).expect("report serialises");
    let mut text = serde_json::to_string_pretty(&value).expect("report serialises");
    text.push('\n');
    text
}

/// A small helper so tests and the CLI can show the same summary lines.
pub fn summary_lines(report: &Report) -> Vec<String> {
    let mut lines = vec![format!(
        "{} {}: {} ({} of {} controls covered)",
        report.name,
        report.as_of,
        report.status,
        report.summary.controls_covered,
        report.summary.controls
    )];
    for control in &report.controls {
        if control.status != "covered" {
            lines.push(format!(
                "  {} {}: {}",
                control.id, control.title, control.status
            ));
        }
    }
    for item in &report.items {
        for finding in &item.findings {
            lines.push(format!(
                "  {} [{}] {}",
                item.id, finding.level, finding.message
            ));
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_convert_to_day_numbers() {
        assert_eq!(parse_date("1970-01-01").unwrap(), 0);
        assert_eq!(parse_date("1970-01-02").unwrap(), 1);
        assert_eq!(
            parse_date("2000-03-01").unwrap() - parse_date("2000-02-28").unwrap(),
            2
        );
        assert_eq!(
            parse_date("2026-10-09").unwrap() - parse_date("2026-09-30").unwrap(),
            9
        );
    }

    #[test]
    fn impossible_dates_are_refused() {
        assert!(parse_date("2026-02-30").is_err());
        assert!(parse_date("2026-13-01").is_err());
        assert!(parse_date("2025-02-29").is_err());
        assert!(parse_date("2024-02-29").is_ok());
        assert!(parse_date("26-10-09").is_err());
    }

    #[test]
    fn path_escape_is_refused() {
        assert!(!is_safe_relative("../secret.txt"));
        assert!(!is_safe_relative("/etc/passwd"));
        assert!(is_safe_relative("files/a.csv"));
    }

    #[test]
    fn kind_is_inferred_from_extension() {
        assert_eq!(infer_kind("a.JSON"), "json");
        assert_eq!(infer_kind("a.xml"), "document");
        assert_eq!(infer_kind("a.yaml"), "policy");
        assert_eq!(infer_kind("server.log"), "log");
        assert_eq!(infer_kind("notes.md"), "text");
    }

    #[test]
    fn sha256_matches_the_known_value() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn duplicate_item_ids_are_refused() {
        let text = "name: x\nitems:\n  - id: a\n    file: f\n    collected: 2026-01-01\n  - id: a\n    file: g\n    collected: 2026-01-01\n";
        assert!(parse_index(text).unwrap_err().contains("used twice"));
    }

    #[test]
    fn bad_collected_date_is_refused_with_the_item_named() {
        let text = "name: x\nitems:\n  - id: a\n    file: f\n    collected: yesterday\n";
        let err = parse_index(text).unwrap_err();
        assert!(err.contains("evidence 'a'"), "{err}");
    }
}
