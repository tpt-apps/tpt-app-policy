use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn example_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/evidence")
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tpt-evidence"))
        .args(args)
        .output()
        .expect("tpt-evidence runs")
}

fn code(output: &Output) -> i32 {
    output.status.code().expect("process exits normally")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tpt-evidence-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Copy a folder, so tests can change files without touching the examples.
fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn process(index: &Path, controls: &Path, as_of: &str, out: &Path) -> Output {
    run(&[
        "process",
        "--evidence",
        index.to_str().unwrap(),
        "--controls",
        controls.to_str().unwrap(),
        "--as-of",
        as_of,
        "--out",
        out.to_str().unwrap(),
    ])
}

fn report(out: &Path) -> serde_json::Value {
    let text = fs::read_to_string(out.join("report.json")).unwrap();
    serde_json::from_str(&text).unwrap()
}

fn control_status(report: &serde_json::Value, id: &str) -> String {
    report["controls"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == id)
        .unwrap_or_else(|| panic!("control {id} missing"))["status"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn example_reports_covered_stale_and_gap_controls() {
    let out = scratch("example");
    let result = process(
        &example_dir().join("evidence.yaml"),
        &example_dir().join("controls.yaml"),
        "2026-10-09",
        &out,
    );
    assert_eq!(code(&result), 20, "{}", stdout(&result));
    let r = report(&out);
    assert_eq!(r["status"], "incomplete");
    assert_eq!(control_status(&r, "AC-2"), "covered");
    assert_eq!(control_status(&r, "CM-3"), "covered");
    assert_eq!(control_status(&r, "CP-9"), "stale");
    assert_eq!(control_status(&r, "IA-5"), "covered");
    assert_eq!(control_status(&r, "IR-4"), "gap");
    assert_eq!(control_status(&r, "SA-9"), "covered");
    assert_eq!(r["summary"]["controls_covered"], 4);
}

#[test]
fn every_output_carries_the_disclaimer() {
    let out = scratch("disclaimer");
    process(
        &example_dir().join("evidence.yaml"),
        &example_dir().join("controls.yaml"),
        "2026-10-09",
        &out,
    );
    let r = report(&out);
    assert!(r["disclaimer"]
        .as_str()
        .unwrap()
        .contains("does not certify"));
    let html = fs::read_to_string(out.join("report.html")).unwrap();
    assert!(html.contains("does not certify"));
}

#[test]
fn html_has_no_scripts_or_external_links() {
    let out = scratch("html");
    process(
        &example_dir().join("evidence.yaml"),
        &example_dir().join("controls.yaml"),
        "2026-10-09",
        &out,
    );
    let html = fs::read_to_string(out.join("report.html")).unwrap();
    assert!(!html.contains("<script"));
    assert!(!html.contains("https://") && !html.contains("http://"));
}

#[test]
fn same_inputs_give_identical_outputs() {
    let first = scratch("repeat-a");
    let second = scratch("repeat-b");
    for out in [&first, &second] {
        process(
            &example_dir().join("evidence.yaml"),
            &example_dir().join("controls.yaml"),
            "2026-10-09",
            out,
        );
    }
    for name in ["report.json", "report.html", "manifest.json"] {
        assert_eq!(
            fs::read(first.join(name)).unwrap(),
            fs::read(second.join(name)).unwrap(),
            "{name}"
        );
    }
}

#[test]
fn a_later_as_of_date_makes_more_evidence_stale() {
    let out = scratch("later");
    process(
        &example_dir().join("evidence.yaml"),
        &example_dir().join("controls.yaml"),
        "2027-06-01",
        &out,
    );
    let r = report(&out);
    assert_eq!(control_status(&r, "CM-3"), "stale");
    assert_eq!(control_status(&r, "AC-2"), "stale");
}

#[test]
fn manifest_verifies_unchanged_files() {
    let out = scratch("verify-ok");
    process(
        &example_dir().join("evidence.yaml"),
        &example_dir().join("controls.yaml"),
        "2026-10-09",
        &out,
    );
    let result = run(&[
        "verify",
        "--manifest",
        out.join("manifest.json").to_str().unwrap(),
        "--root",
        example_dir().to_str().unwrap(),
    ]);
    assert_eq!(code(&result), 0, "{}", stdout(&result));
    assert!(stdout(&result).contains("all 6 files match the manifest"));
}

#[test]
fn changed_file_fails_verification_with_exit_10() {
    let copy = scratch("tampered");
    copy_dir(&example_dir(), &copy);
    let out = scratch("tampered-out");
    process(
        &copy.join("evidence.yaml"),
        &copy.join("controls.yaml"),
        "2026-10-09",
        &out,
    );
    fs::write(
        copy.join("files/change-ticket.json"),
        "{\"ticket\": \"CHG-9999\"}",
    )
    .unwrap();
    let result = run(&[
        "verify",
        "--manifest",
        out.join("manifest.json").to_str().unwrap(),
        "--root",
        copy.to_str().unwrap(),
    ]);
    assert_eq!(code(&result), 10);
    assert!(
        stdout(&result).contains("changed  files/change-ticket.json"),
        "{}",
        stdout(&result)
    );
}

#[test]
fn edited_manifest_is_detected() {
    let out = scratch("edited");
    process(
        &example_dir().join("evidence.yaml"),
        &example_dir().join("controls.yaml"),
        "2026-10-09",
        &out,
    );
    let path = out.join("manifest.json");
    let text = fs::read_to_string(&path).unwrap();
    let mut value: serde_json::Value = serde_json::from_str(&text).unwrap();
    value["entries"][0]["sha256"] = serde_json::json!("0".repeat(64));
    fs::write(&path, serde_json::to_string_pretty(&value).unwrap()).unwrap();
    let result = run(&[
        "verify",
        "--manifest",
        path.to_str().unwrap(),
        "--root",
        example_dir().to_str().unwrap(),
    ]);
    assert_eq!(code(&result), 10);
    assert!(
        stdout(&result).contains("manifest was edited"),
        "{}",
        stdout(&result)
    );
}

#[test]
fn declared_hash_that_no_longer_matches_is_an_item_error() {
    let copy = scratch("declared");
    copy_dir(&example_dir(), &copy);
    fs::write(copy.join("files/access-review.csv"), "user,role\nx,y\n").unwrap();
    let out = scratch("declared-out");
    let result = process(
        &copy.join("evidence.yaml"),
        &copy.join("controls.yaml"),
        "2026-10-09",
        &out,
    );
    assert_eq!(code(&result), 20);
    let r = report(&out);
    let item = r["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["id"] == "access-review-q3")
        .unwrap();
    assert_eq!(item["status"], "error");
    assert!(item["findings"][0]["message"]
        .as_str()
        .unwrap()
        .contains("changed since its hash"));
}

#[test]
fn path_that_leaves_the_folder_is_an_item_error_not_a_read() {
    let dir = scratch("escape");
    let index = dir.join("evidence.yaml");
    fs::write(
        &index,
        "name: escape\nitems:\n  - id: outside\n    file: ../secret.txt\n    collected: 2026-09-01\n    controls: [C1]\n",
    )
    .unwrap();
    let controls = dir.join("controls.yaml");
    fs::write(
        &controls,
        "framework: f\ncontrols:\n  - id: C1\n    title: t\n",
    )
    .unwrap();
    let out = scratch("escape-out");
    let result = process(&index, &controls, "2026-10-09", &out);
    assert_eq!(code(&result), 20);
    let r = report(&out);
    assert!(r["items"][0]["findings"][0]["message"]
        .as_str()
        .unwrap()
        .contains("inside the index folder"));
}

#[test]
fn missing_file_is_reported_per_item() {
    let dir = scratch("missing");
    let index = dir.join("evidence.yaml");
    fs::write(
        &index,
        "name: missing\nitems:\n  - id: gone\n    file: nothing.csv\n    collected: 2026-09-01\n    controls: [C1]\n",
    )
    .unwrap();
    let controls = dir.join("controls.yaml");
    fs::write(
        &controls,
        "framework: f\ncontrols:\n  - id: C1\n    title: t\n",
    )
    .unwrap();
    let out = scratch("missing-out");
    assert_eq!(code(&process(&index, &controls, "2026-10-09", &out)), 20);
    assert_eq!(report(&out)["items"][0]["status"], "error");
}

#[test]
fn impossible_as_of_date_exits_2() {
    let out = scratch("bad-date");
    let result = process(
        &example_dir().join("evidence.yaml"),
        &example_dir().join("controls.yaml"),
        "2026-02-30",
        &out,
    );
    assert_eq!(code(&result), 2);
    assert!(String::from_utf8_lossy(&result.stderr).contains("not a real date"));
}

#[test]
fn unknown_control_in_an_item_is_a_warning() {
    let dir = scratch("unknown-control");
    let index = dir.join("evidence.yaml");
    fs::write(
        &index,
        "name: u\nitems:\n  - id: a\n    file: f.txt\n    collected: 2026-09-01\n    controls: [NOPE]\n",
    )
    .unwrap();
    fs::write(dir.join("f.txt"), "hello\n").unwrap();
    let controls = dir.join("controls.yaml");
    fs::write(
        &controls,
        "framework: f\ncontrols:\n  - id: C1\n    title: t\n",
    )
    .unwrap();
    let out = scratch("unknown-control-out");
    process(&index, &controls, "2026-10-09", &out);
    let r = report(&out);
    assert_eq!(r["items"][0]["status"], "warning");
    assert!(r["items"][0]["findings"][0]["message"]
        .as_str()
        .unwrap()
        .contains("NOPE"));
}

#[test]
fn doctor_passes() {
    let result = run(&["doctor"]);
    assert_eq!(code(&result), 0, "{}", stdout(&result));
    assert!(stdout(&result).contains("all checks passed"));
}
