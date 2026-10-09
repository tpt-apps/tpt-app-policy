use std::path::PathBuf;
use std::process::{Command, Output};

use serde_json::json;
use tpt_document::xml::xml_to_value;
use tpt_document::{check_document, read_document, DocFormat, Parsed, Verdict};
use tpt_policy_core::parse_policy;
use tpt_schema::parse_schema;

fn example(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/documents")
        .join(relative)
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tpt-document"))
        .args(args)
        .output()
        .expect("tpt-document runs")
}

fn code(output: &Output) -> i32 {
    output.status.code().expect("process exits normally")
}

fn invoice_schema() -> tpt_schema::Schema {
    let text = std::fs::read_to_string(example("templates/invoice.schema.yaml")).unwrap();
    parse_schema(&text).unwrap()
}

fn invoice_policy() -> tpt_policy_core::Policy {
    let text = std::fs::read_to_string(example("templates/invoice.policy.yaml")).unwrap();
    parse_policy(&text).unwrap()
}

// --- XML conversion rules ---------------------------------------------------

#[test]
fn root_children_become_the_fields() {
    let v = xml_to_value("<invoice><total>10</total></invoice>").unwrap();
    assert_eq!(v, json!({"total": "10"}));
}

#[test]
fn attributes_are_prefixed_with_at_sign() {
    let v = xml_to_value(r#"<doc><supplier id="S-17"><name>Acme</name></supplier></doc>"#).unwrap();
    assert_eq!(v, json!({"supplier": {"@id": "S-17", "name": "Acme"}}));
}

#[test]
fn repeated_elements_become_a_list() {
    let v = xml_to_value("<doc><line>a</line><line>b</line><line>c</line></doc>").unwrap();
    assert_eq!(v, json!({"line": ["a", "b", "c"]}));
}

#[test]
fn empty_elements_are_null_and_text_is_trimmed() {
    let v = xml_to_value("<doc><empty/><padded>  hi  </padded></doc>").unwrap();
    assert_eq!(v, json!({"empty": null, "padded": "hi"}));
}

#[test]
fn entities_are_decoded() {
    let v = xml_to_value("<doc><name>Smith &amp; Sons &lt;Ltd&gt;</name></doc>").unwrap();
    assert_eq!(v, json!({"name": "Smith & Sons <Ltd>"}));
}

#[test]
fn text_beside_children_is_kept_under_hash_text() {
    let v =
        xml_to_value("<doc><price currency=\"NZD\">12.50<note>net</note></price></doc>").unwrap();
    assert_eq!(
        v,
        json!({"price": {"@currency": "NZD", "#text": "12.50", "note": "net"}})
    );
}

#[test]
fn malformed_xml_is_an_error_with_a_message() {
    let err = xml_to_value("<doc><a></doc>").unwrap_err();
    assert!(err.message.contains("not well formed"), "{}", err.message);
    assert!(xml_to_value("<doc>")
        .unwrap_err()
        .message
        .contains("never closed"));
    assert!(xml_to_value("just text").is_err());
}

#[test]
fn root_with_no_fields_is_refused() {
    let err = xml_to_value("<doc>text only</doc>").unwrap_err();
    assert!(err.message.contains("no fields"), "{}", err.message);
}

// --- pipeline ----------------------------------------------------------------

#[test]
fn xml_invoice_over_limit_is_review_with_the_policy_decision() {
    let parsed = read_document(&example("invoice.xml")).unwrap();
    let result = check_document(&invoice_schema(), Some(&invoice_policy()), &parsed);
    assert_eq!(result.verdict, Verdict::Review);
    let ev = result.evaluation.expect("policy ran");
    assert_eq!(ev.approvers, vec!["finance".to_string()]);
}

#[test]
fn schema_failure_is_fail_and_policy_does_not_run() {
    let parsed = read_document(&example("invoice-bad.json")).unwrap();
    let result = check_document(&invoice_schema(), Some(&invoice_policy()), &parsed);
    assert_eq!(result.verdict, Verdict::Fail);
    assert!(result.evaluation.is_none());
    assert!(result.schema_errors.iter().any(|e| e.field == "tax"));
}

#[test]
fn malformed_document_is_fail_with_the_parse_error() {
    let parsed = read_document(&example("broken.xml")).unwrap();
    assert!(matches!(
        parsed,
        Parsed::Malformed {
            format: DocFormat::Xml,
            ..
        }
    ));
    let result = check_document(&invoice_schema(), None, &parsed);
    assert_eq!(result.verdict, Verdict::Fail);
    assert_eq!(result.schema_errors[0].field, "(document)");
}

#[test]
fn xml_text_values_are_checked_as_text_so_numbers_still_compare() {
    // The XML carries 5750.00 as text. The policy rule `total >= 5000` must still match.
    let parsed = read_document(&example("invoice.xml")).unwrap();
    let result = check_document(&invoice_schema(), Some(&invoice_policy()), &parsed);
    assert!(result
        .evaluation
        .unwrap()
        .matched_rules
        .contains(&"large-invoice".to_string()));
}

#[test]
fn json_document_without_policy_passes() {
    let parsed = read_document(&example("purchase-order.json")).unwrap();
    let schema = parse_schema(
        &std::fs::read_to_string(example("templates/purchase-order.schema.yaml")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        check_document(&schema, None, &parsed).verdict,
        Verdict::Pass
    );
}

#[test]
fn unknown_extension_is_an_error() {
    let dir = std::env::temp_dir().join(format!("tpt-doc-ext-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("notes.txt");
    std::fs::write(&path, "hello").unwrap();
    assert!(read_document(&path).is_err());
}

// --- CLI ---------------------------------------------------------------------

#[test]
fn cli_exit_code_is_the_worst_verdict() {
    let out = run(&[
        "validate",
        example("invoice.xml").to_str().unwrap(),
        example("invoice.xml").to_str().unwrap(),
        "--schema",
        example("templates/invoice.schema.yaml").to_str().unwrap(),
        "--policy",
        example("templates/invoice.policy.yaml").to_str().unwrap(),
    ]);
    assert_eq!(code(&out), 20, "two REVIEW documents give 20");
}

#[test]
fn cli_pass_exits_0_and_prints_pass() {
    let out = run(&[
        "validate",
        example("shipment.xml").to_str().unwrap(),
        "--schema",
        example("templates/shipping.schema.yaml").to_str().unwrap(),
    ]);
    assert_eq!(code(&out), 0);
    assert!(String::from_utf8_lossy(&out.stdout).starts_with("PASS"));
}

#[test]
fn cli_fail_exits_30() {
    let out = run(&[
        "validate",
        example("broken.xml").to_str().unwrap(),
        "--schema",
        example("templates/invoice.schema.yaml").to_str().unwrap(),
    ]);
    assert_eq!(code(&out), 30);
}

#[test]
fn cli_bad_schema_exits_2() {
    let dir = std::env::temp_dir().join(format!("tpt-doc-schema-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let schema = dir.join("bad.yaml");
    std::fs::write(&schema, "schema: x\nfields:\n  a:\n    type: date\n").unwrap();
    let out = run(&[
        "validate",
        example("shipment.xml").to_str().unwrap(),
        "--schema",
        schema.to_str().unwrap(),
    ]);
    assert_eq!(code(&out), 2);
}

#[test]
fn cli_writes_one_result_file_per_document() {
    let dir = std::env::temp_dir().join(format!("tpt-doc-out-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let out = run(&[
        "validate",
        example("invoice.xml").to_str().unwrap(),
        "--schema",
        example("templates/invoice.schema.yaml").to_str().unwrap(),
        "--policy",
        example("templates/invoice.policy.yaml").to_str().unwrap(),
        "--out",
        dir.to_str().unwrap(),
    ]);
    assert_eq!(code(&out), 20);
    let text = std::fs::read_to_string(dir.join("invoice.result.json")).expect("result written");
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["verdict"], "REVIEW");
    assert_eq!(v["document"]["format"], "xml");
    assert_eq!(v["policy"]["name"], "invoice-approval");
}

#[test]
fn cli_html_writes_a_self_contained_report() {
    let dir = std::env::temp_dir().join(format!("tpt-doc-html-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let out = run(&[
        "validate",
        example("invoice.xml").to_str().unwrap(),
        example("invoice-bad.json").to_str().unwrap(),
        "--schema",
        example("templates/invoice.schema.yaml").to_str().unwrap(),
        "--policy",
        example("templates/invoice.policy.yaml").to_str().unwrap(),
        "--out",
        dir.to_str().unwrap(),
        "--html",
    ]);
    assert_eq!(
        code(&out),
        30,
        "one document failed, so the worst verdict is FAIL"
    );
    let html = std::fs::read_to_string(dir.join("report.html")).expect("report written");
    assert!(html.contains("Document validation report"));
    assert!(html.contains("invoice-bad.json"));
    assert!(html.contains("invoice.xml"));
    assert!(!html.contains("<script"), "no scripts");
    assert!(!html.contains("https://"), "no external links");
}

#[test]
fn cli_html_needs_an_out_folder() {
    let out = run(&[
        "validate",
        example("invoice.xml").to_str().unwrap(),
        "--schema",
        example("templates/invoice.schema.yaml").to_str().unwrap(),
        "--html",
    ]);
    assert_eq!(code(&out), 2, "clap rejects --html without --out");
}

#[test]
fn cli_result_is_byte_identical_across_runs() {
    let read = || {
        let out = run(&[
            "validate",
            example("invoice.xml").to_str().unwrap(),
            "--schema",
            example("templates/invoice.schema.yaml").to_str().unwrap(),
            "--policy",
            example("templates/invoice.policy.yaml").to_str().unwrap(),
        ]);
        out.stdout
    };
    assert_eq!(read(), read());
}

#[test]
fn doctor_passes() {
    assert_eq!(code(&run(&["doctor"])), 0);
}

#[test]
fn schema_can_check_an_attribute_and_a_list_item() {
    // The second <line> carries an attribute, so it is an object: check its @sku.
    let schema = parse_schema(
        "schema: t
fields:
  supplier.@id:
    type: string
    required: true
  line.1.@sku:
    type: string
    required: true
",
    )
    .unwrap();
    let parsed = read_document(&example("invoice.xml")).unwrap();
    let result = check_document(&schema, None, &parsed);
    assert_eq!(result.verdict, Verdict::Pass, "{:?}", result.schema_errors);
}
