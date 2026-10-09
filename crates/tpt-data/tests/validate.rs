//! End-to-end tests for `tpt-data`, run against the real binary and the
//! example data in examples/data.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn example(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/data")
        .join(name)
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tpt-data-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tpt-data"))
        .args(args)
        .output()
        .expect("tpt-data runs")
}

fn code(output: &Output) -> i32 {
    output.status.code().expect("process exits normally")
}

fn validate(input: &Path, out: &Path, policy: bool) -> Output {
    let schema = example("customer.schema.yaml");
    let mut args = vec![
        "validate".to_string(),
        input.to_string_lossy().into_owned(),
        "--schema".to_string(),
        schema.to_string_lossy().into_owned(),
        "--out".to_string(),
        out.to_string_lossy().into_owned(),
    ];
    if policy {
        args.push("--policy".to_string());
        args.push(example("credit.policy.yaml").to_string_lossy().into_owned());
    }
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    run(&refs)
}

fn summary(out: &Path) -> serde_json::Value {
    let text = std::fs::read_to_string(out.join("summary.json")).expect("summary written");
    serde_json::from_str(&text).expect("summary is JSON")
}

#[test]
fn csv_run_counts_valid_and_invalid_and_exits_10() {
    let out = scratch("csv");
    let result = validate(&example("customers.csv"), &out, true);
    assert_eq!(code(&result), 10);

    let s = summary(&out);
    assert_eq!(s["result"]["processed"], 7);
    assert_eq!(s["result"]["valid"], 2);
    assert_eq!(s["result"]["invalid"], 5);
    assert_eq!(s["result"]["decisions"]["review"], 1);
    assert_eq!(s["result"]["decisions"]["approved"], 1);
    assert_eq!(s["schema"]["name"], "customer");
    assert_eq!(s["policy"]["name"], "credit-review");
}

#[test]
fn error_report_gives_row_field_and_reason() {
    let out = scratch("report");
    validate(&example("customers.csv"), &out, false);
    let text = std::fs::read_to_string(out.join("errors.txt")).unwrap();
    assert!(
        text.contains("Row 2:\n  email: 'bad-email' does not match the pattern"),
        "{text}"
    );
    assert!(
        text.contains("Row 3:\n  country: 'FR' is not one of"),
        "{text}"
    );
    assert!(
        text.contains("Row 4:\n  customer_id: duplicate value 'C001' (first seen at row 1)"),
        "{text}"
    );
    assert!(
        text.contains("Row 5:\n  credit_limit: 250000 is above the maximum"),
        "{text}"
    );
    assert!(text.contains("Row 6:\n  email: is required"), "{text}");
}

#[test]
fn csv_valid_output_keeps_the_input_columns() {
    let out = scratch("csv-valid");
    validate(&example("customers.csv"), &out, false);
    let text = std::fs::read_to_string(out.join("valid.csv")).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        lines[0],
        "customer_id,email,country,credit_limit,address.postcode"
    );
    assert_eq!(lines.len(), 3, "header plus two valid rows: {text}");
    assert!(lines[1].starts_with("C001,ana@example.com,NZ,5000"));
}

#[test]
fn invalid_output_is_jsonl_with_row_and_errors() {
    let out = scratch("invalid");
    validate(&example("customers.csv"), &out, false);
    let text = std::fs::read_to_string(out.join("invalid.jsonl")).unwrap();
    let first: serde_json::Value = serde_json::from_str(text.lines().next().unwrap()).unwrap();
    assert_eq!(first["row"], 2);
    assert_eq!(first["errors"][0]["field"], "email");
    assert_eq!(first["record"]["customer_id"], "C002");
}

#[test]
fn json_array_is_strictly_typed_and_written_back_as_json() {
    let out = scratch("json");
    let input = out.join("in.json");
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(
        &input,
        r#"[
  {"customer_id":"C1","email":"a@b.co","country":"NZ","credit_limit":5000},
  {"customer_id":"C2","email":"c@d.co","country":"AU","credit_limit":"2000"}
]"#,
    )
    .unwrap();
    let result = validate(&input, &out, false);
    assert_eq!(code(&result), 10);
    let s = summary(&out);
    assert_eq!(s["result"]["valid"], 1);
    assert_eq!(s["result"]["invalid"], 1);
    let valid: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(out.join("valid.json")).unwrap()).unwrap();
    assert_eq!(valid.as_array().unwrap().len(), 1);
}

#[test]
fn jsonl_skips_blank_lines_and_counts_bad_lines_as_invalid() {
    let out = scratch("jsonl");
    let result = validate(&example("customers.jsonl"), &out, false);
    assert_eq!(code(&result), 10);
    let s = summary(&out);
    assert_eq!(s["result"]["processed"], 3);
    assert_eq!(s["result"]["invalid"], 1);

    let input = out.join("bad.jsonl");
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(
        &input,
        "{\"customer_id\":\"C1\",\"email\":\"a@b.co\",\"country\":\"NZ\"}\n{not json\n[1,2]\n",
    )
    .unwrap();
    let out2 = scratch("jsonl-bad");
    let result = validate(&input, &out2, false);
    assert_eq!(code(&result), 10, "bad lines do not stop the run");
    let s = summary(&out2);
    assert_eq!(s["result"]["processed"], 3);
    assert_eq!(s["result"]["invalid"], 2);
    let text = std::fs::read_to_string(out2.join("errors.txt")).unwrap();
    assert!(text.contains("(record):"), "{text}");
}

#[test]
fn all_valid_input_exits_0() {
    let out = scratch("all-valid");
    let input = out.join("ok.jsonl");
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(
        &input,
        "{\"customer_id\":\"C1\",\"email\":\"a@b.co\",\"country\":\"NZ\"}\n",
    )
    .unwrap();
    let out2 = scratch("all-valid-out");
    assert_eq!(code(&validate(&input, &out2, false)), 0);
}

#[test]
fn outputs_are_byte_identical_across_runs() {
    let a = scratch("det-a");
    let b = scratch("det-b");
    validate(&example("customers.csv"), &a, true);
    validate(&example("customers.csv"), &b, true);
    for name in ["valid.csv", "invalid.jsonl", "errors.txt", "summary.json"] {
        assert_eq!(
            std::fs::read(a.join(name)).unwrap(),
            std::fs::read(b.join(name)).unwrap(),
            "{name} differs between runs"
        );
    }
}

#[test]
fn bad_json_array_is_an_input_error() {
    let out = scratch("bad-array");
    let input = out.join("broken.json");
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(&input, "[{\"a\":1},").unwrap();
    let out2 = scratch("bad-array-out");
    assert_eq!(code(&validate(&input, &out2, false)), 3);
}

#[test]
fn bad_schema_exits_2_and_names_the_field() {
    let out = scratch("bad-schema");
    std::fs::create_dir_all(&out).unwrap();
    let schema = out.join("bad.yaml");
    std::fs::write(&schema, "schema: x\nfields:\n  a:\n    type: date\n").unwrap();
    let result = run(&[
        "validate",
        example("customers.csv").to_str().unwrap(),
        "--schema",
        schema.to_str().unwrap(),
        "--out",
        out.join("o").to_str().unwrap(),
    ]);
    assert_eq!(code(&result), 2);
    assert!(String::from_utf8_lossy(&result.stderr).contains("fields.a.type"));
}

#[test]
fn unknown_extension_needs_format_flag() {
    let out = scratch("ext");
    std::fs::create_dir_all(&out).unwrap();
    let input = out.join("data.txt");
    std::fs::write(&input, "customer_id\nC1\n").unwrap();
    let result = run(&[
        "validate",
        input.to_str().unwrap(),
        "--schema",
        example("customer.schema.yaml").to_str().unwrap(),
        "--out",
        out.join("o").to_str().unwrap(),
    ]);
    assert_eq!(code(&result), 2);
    assert!(String::from_utf8_lossy(&result.stderr).contains("--format"));
}

#[test]
fn doctor_passes() {
    assert_eq!(code(&run(&["doctor"])), 0);
}

#[test]
fn html_flag_writes_a_report_with_each_invalid_row() {
    let out = scratch("html");
    let result = run(&[
        "validate",
        example("customers.csv").to_str().unwrap(),
        "--schema",
        example("customer.schema.yaml").to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
        "--html",
    ]);
    assert_eq!(code(&result), 10);
    let html = std::fs::read_to_string(out.join("report.html")).expect("report written");
    assert_eq!(html.matches("<article>").count(), 5);
    assert!(html.contains("<h3>Row 4</h3>"));
    assert!(!html.contains("<script"));
}

#[test]
fn no_html_file_without_the_flag() {
    let out = scratch("no-html");
    validate(&example("customers.csv"), &out, false);
    assert!(!out.join("report.html").exists());
}
