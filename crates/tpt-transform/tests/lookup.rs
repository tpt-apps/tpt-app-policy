use std::path::PathBuf;
use std::process::Command;

use tpt_data_core::{open, Format};
use tpt_transform::{parse_pipeline, run, Table};

fn example_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/transform/lookup")
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "tpt-transform-lookup-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn unknown_codes_are_rejected_and_known_ones_gain_the_table_values() {
    let out = scratch("cli");
    let output = Command::new(env!("CARGO_BIN_EXE_tpt-transform"))
        .args(["run"])
        .arg(example_dir().join("orders.csv"))
        .arg("--pipeline")
        .arg(example_dir().join("order-lookup.pipeline.yaml"))
        .arg("--out")
        .arg(&out)
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(10),
        "one order has an unknown country"
    );

    let transformed = std::fs::read_to_string(out.join("transformed.csv")).unwrap();
    assert!(
        transformed.contains("1001,NZ,250.00,New Zealand,Oceania"),
        "{transformed}"
    );
    assert!(
        transformed.contains("1002,US,40.50,United States,Americas"),
        "{transformed}"
    );
    assert!(
        !transformed.contains("1003"),
        "the unknown country is not kept"
    );

    let rejected = std::fs::read_to_string(out.join("rejected.jsonl")).unwrap();
    assert!(
        rejected.contains("'XX' is not in the table 'countries.csv'"),
        "{rejected}"
    );
}

#[test]
fn on_missing_skip_keeps_the_record_unchanged() {
    let dir = scratch("skip");
    std::fs::copy(
        example_dir().join("countries.csv"),
        dir.join("countries.csv"),
    )
    .unwrap();
    let text = "pipeline:\n  - lookup:\n      field: country\n      table: countries.csv\n      key: code\n      add:\n        country_name: name\n";
    let mut pipeline = parse_pipeline(text).unwrap();
    pipeline.load_tables(&dir).unwrap();
    let input = open(&example_dir().join("orders.csv"), Format::Csv).unwrap();
    let result = run(&pipeline, input);
    assert!(result.rejected.is_empty(), "skip never rejects");
    assert_eq!(result.kept.len(), 4);
    let unknown = result.kept.iter().find(|(n, _)| *n == 3).unwrap();
    assert!(
        unknown.1.get("country_name").is_none(),
        "no value is added for an unknown code"
    );
    assert_eq!(unknown.1["order_id"], "1003");
}

#[test]
fn a_table_with_a_repeated_key_is_an_error() {
    let dir = scratch("dupe");
    let table = dir.join("dupes.csv");
    std::fs::write(&table, "code,name\nNZ,New Zealand\nNZ,Aotearoa\n").unwrap();
    let why = Table::load(&table, "code").unwrap_err();
    assert!(why.contains("'NZ' appears more than once"), "{why}");
}

#[test]
fn a_missing_table_is_reported_before_any_record_is_read() {
    let dir = scratch("missing");
    let text = "pipeline:\n  - lookup:\n      field: country\n      table: nope.csv\n      key: code\n      add:\n        name: name\n";
    let mut pipeline = parse_pipeline(text).unwrap();
    let why = pipeline.load_tables(&dir).unwrap_err();
    assert!(why.starts_with("step 1 (lookup):"), "{why}");
}

#[test]
fn a_lookup_with_no_add_fields_is_invalid() {
    let text = "pipeline:\n  - lookup:\n      field: country\n      table: t.csv\n      key: code\n      add: {}\n";
    let why = parse_pipeline(text).unwrap_err();
    assert!(why.contains("'add' is empty"), "{why}");
}

#[test]
fn the_table_path_is_relative_to_the_pipeline_file() {
    let dir = scratch("cli-relative");
    let pipeline_path = dir.join("p.yaml");
    std::fs::write(
        &pipeline_path,
        "pipeline:\n  - lookup:\n      field: country\n      table: missing.csv\n      key: code\n      add:\n        name: name\n",
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_tpt-transform"))
        .args(["check", "--pipeline"])
        .arg(&pipeline_path)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("missing.csv"), "{stderr}");
}

#[test]
fn a_broken_json_table_is_an_error_not_a_partial_table() {
    let dir = scratch("broken-json");
    let table = dir.join("codes.json");
    std::fs::write(
        &table,
        r#"[{"code": "NZ", "name": "New Zealand"}, {"code": "AU""#,
    )
    .unwrap();
    let why = Table::load(&table, "code").unwrap_err();
    assert!(why.contains("ends before the array is closed"), "{why}");
}
