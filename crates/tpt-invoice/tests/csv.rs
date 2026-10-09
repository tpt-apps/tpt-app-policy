use std::path::PathBuf;
use std::process::Command;

use tpt_document::Parsed;
use tpt_invoice::{check_invoice, read_invoices, Verdict};
use tpt_schema::parse_schema;

fn example(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/invoices")
        .join(relative)
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tpt-invoice-csv-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn schema() -> tpt_schema::Schema {
    parse_schema(&std::fs::read_to_string(example("invoice.schema.yaml")).unwrap()).unwrap()
}

#[test]
fn rows_with_the_same_invoice_number_form_one_invoice() {
    let found = read_invoices(&example("batch.csv")).unwrap();
    assert_eq!(found.len(), 3, "three invoices in the batch");
    assert_eq!(
        found[0].label,
        format!("{} row 1", example("batch.csv").display())
    );
    assert_eq!(found[0].out_stem, "batch.row-1");
    assert_eq!(found[1].out_stem, "batch.row-3");
    assert_eq!(found[2].out_stem, "batch.row-4");
}

#[test]
fn a_grouped_invoice_has_one_line_per_row() {
    let found = read_invoices(&example("batch.csv")).unwrap();
    let Parsed::Ok { value, .. } = &found[0].parsed else {
        panic!("INV-2001 should parse");
    };
    assert_eq!(value["lines"].as_array().unwrap().len(), 2);
    assert_eq!(value["lines"][0]["description"], "Office chair");
    assert!(
        value.get("line").is_none(),
        "line columns are not invoice fields"
    );
}

#[test]
fn csv_invoices_are_checked_like_any_other() {
    let found = read_invoices(&example("batch.csv")).unwrap();
    let verdicts: Vec<Verdict> = found
        .iter()
        .map(|i| check_invoice(&schema(), None, None, None, &i.parsed).verdict)
        .collect();
    assert_eq!(verdicts[0], Verdict::Pass, "INV-2001 adds up");
    assert_eq!(verdicts[1], Verdict::Reject, "INV-2002 total is wrong");
    assert_eq!(verdicts[2], Verdict::Reject, "INV-2003 rows disagree");

    let total = check_invoice(&schema(), None, None, None, &found[1].parsed);
    assert!(total.checks.iter().any(|c| c.name == "total" && !c.passed));
}

#[test]
fn rows_that_disagree_on_a_header_field_name_the_field() {
    let found = read_invoices(&example("batch.csv")).unwrap();
    let Parsed::Malformed { message, .. } = &found[2].parsed else {
        panic!("INV-2003 should be malformed");
    };
    assert!(message.contains("'currency'"), "{message}");
    assert!(message.contains("rows 4 and 5"), "{message}");
}

#[test]
fn json_files_still_give_one_invoice_named_by_the_file() {
    let found = read_invoices(&example("invoice-ok.json")).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].out_stem, "invoice-ok");
}

#[test]
fn cli_writes_one_result_per_csv_invoice() {
    let out = scratch("cli");
    let dir = scratch("cli-input");
    let input = dir.join("batch.csv");
    std::fs::copy(example("batch.csv"), &input).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_tpt-invoice"))
        .args(["validate", "--schema"])
        .arg(example("invoice.schema.yaml"))
        .arg(&input)
        .arg("--out")
        .arg(&out)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(30), "one invoice is rejected");
    for name in ["batch.row-1", "batch.row-3", "batch.row-4"] {
        assert!(out.join(format!("{name}.result.json")).exists(), "{name}");
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("batch.csv row 3"), "{stdout}");
}
