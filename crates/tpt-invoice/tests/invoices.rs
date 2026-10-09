use std::path::PathBuf;
use std::process::{Command, Output};

use serde_json::{json, Value};
use tpt_document::{DocFormat, Parsed};
use tpt_invoice::{check_invoice, Ledger, Suppliers, Verdict};
use tpt_policy_core::parse_policy;
use tpt_schema::parse_schema;

fn example(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/invoices")
        .join(relative)
}

fn schema() -> tpt_schema::Schema {
    parse_schema(&std::fs::read_to_string(example("invoice.schema.yaml")).unwrap()).unwrap()
}

fn policy() -> tpt_policy_core::Policy {
    parse_policy(&std::fs::read_to_string(example("invoice.policy.yaml")).unwrap()).unwrap()
}

fn suppliers() -> Suppliers {
    Suppliers::parse(&std::fs::read_to_string(example("suppliers.json")).unwrap()).unwrap()
}

/// A valid NZD invoice with one line. Each test changes one field.
fn base() -> Value {
    json!({
        "invoice_number": "T-1",
        "issue_date": "2026-01-01",
        "supplier": { "name": "Acme", "tax_id": "123-456-789" },
        "currency": "NZD",
        "lines": [{ "quantity": 2, "unit_price": 50.0, "amount": 100.0 }],
        "subtotal": 100.0,
        "tax_rate": 0.15,
        "tax": 15.0,
        "total": 115.0
    })
}

fn run_with(mut doc: Value, change: impl FnOnce(&mut Value)) -> tpt_invoice::InvoiceResult {
    change(&mut doc);
    let parsed = Parsed::Ok {
        format: DocFormat::Json,
        value: doc,
    };
    check_invoice(
        &schema(),
        Some(&policy()),
        Some(&suppliers()),
        None,
        &parsed,
    )
}

fn failed_checks(result: &tpt_invoice::InvoiceResult) -> Vec<&'static str> {
    result
        .checks
        .iter()
        .filter(|c| !c.passed)
        .map(|c| c.name)
        .collect()
}

#[test]
fn a_correct_invoice_passes() {
    let result = run_with(base(), |_| {});
    assert_eq!(result.verdict, Verdict::Pass, "{:?}", result.checks);
    assert!(failed_checks(&result).is_empty());
}

#[test]
fn line_amount_must_equal_quantity_times_price() {
    let r = run_with(base(), |d| d["lines"][0]["amount"] = json!(90.0));
    assert_eq!(r.verdict, Verdict::Reject);
    assert_eq!(failed_checks(&r), vec!["line items", "subtotal"]);
}

#[test]
fn subtotal_must_equal_the_sum_of_lines() {
    let r = run_with(base(), |d| d["subtotal"] = json!(120.0));
    assert!(failed_checks(&r).contains(&"subtotal"));
}

#[test]
fn tax_must_match_the_stated_rate() {
    let r = run_with(base(), |d| d["tax"] = json!(20.0));
    assert!(failed_checks(&r).contains(&"tax"));
}

#[test]
fn total_must_equal_subtotal_plus_tax() {
    let r = run_with(base(), |d| d["total"] = json!(120.0));
    assert!(failed_checks(&r).contains(&"total"));
}

#[test]
fn unapproved_supplier_is_rejected() {
    let r = run_with(base(), |d| d["supplier"]["tax_id"] = json!("000-000-000"));
    assert!(failed_checks(&r).contains(&"supplier"));
}

#[test]
fn no_tax_rate_skips_the_rate_check() {
    let r = run_with(base(), |d| {
        d.as_object_mut().unwrap().remove("tax_rate");
    });
    assert_eq!(r.verdict, Verdict::Pass, "{:?}", r.checks);
    let tax = r.checks.iter().find(|c| c.name == "tax").unwrap();
    assert!(tax.message.contains("not recomputed"));
}

#[test]
fn one_cent_of_rounding_is_allowed_and_more_is_not() {
    let within = run_with(base(), |d| d["total"] = json!(115.004));
    assert!(failed_checks(&within).is_empty(), "0.004 is within a cent");
    let beyond = run_with(base(), |d| d["total"] = json!(115.02));
    assert!(failed_checks(&beyond).contains(&"total"));
}

#[test]
fn zero_lines_is_rejected() {
    let r = run_with(base(), |d| d["lines"] = json!([]));
    assert!(failed_checks(&r).contains(&"line items"));
}

#[test]
fn schema_failure_rejects_and_skips_the_checks() {
    let r = run_with(base(), |d| {
        d.as_object_mut().unwrap().remove("invoice_number");
    });
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(r.checks.is_empty());
    assert!(r.schema_errors.iter().any(|e| e.field == "invoice_number"));
}

#[test]
fn large_invoice_is_review_from_the_policy() {
    let r = run_with(base(), |d| {
        d["lines"][0]["amount"] = json!(5000.0);
        d["lines"][0]["unit_price"] = json!(2500.0);
        d["subtotal"] = json!(5000.0);
        d["tax"] = json!(750.0);
        d["total"] = json!(5750.0);
    });
    assert_eq!(r.verdict, Verdict::Review);
    assert_eq!(r.evaluation.unwrap().approvers, vec!["finance".to_string()]);
}

#[test]
fn policy_rejection_is_reject() {
    let p = parse_policy(
        "policy: strict\nrules:\n  - id: no\n    when:\n      currency: { equals: NZD }\n    then:\n      decision: rejected\n",
    )
    .unwrap();
    let parsed = Parsed::Ok {
        format: DocFormat::Json,
        value: base(),
    };
    let r = check_invoice(&schema(), Some(&p), Some(&suppliers()), None, &parsed);
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn xml_invoice_with_several_lines_passes() {
    let xml = "<invoice><invoice_number>X</invoice_number><issue_date>2026-01-01</issue_date>\
        <supplier><name>A</name><tax_id>123-456-789</tax_id></supplier><currency>NZD</currency>\
        <lines><line><quantity>2</quantity><unit_price>50.00</unit_price><amount>100.00</amount></line>\
        <line><quantity>1</quantity><unit_price>15</unit_price><amount>15</amount></line></lines>\
        <subtotal>115.00</subtotal><tax_rate>0.15</tax_rate><tax>17.25</tax><total>132.25</total></invoice>";
    let value = tpt_document::xml::xml_to_value(xml).unwrap();
    let parsed = Parsed::Ok {
        format: DocFormat::Xml,
        value,
    };
    let r = check_invoice(&schema(), None, Some(&suppliers()), None, &parsed);
    assert_eq!(r.verdict, Verdict::Pass, "{:?}", r.checks);
}

#[test]
fn ledger_catches_a_duplicate_after_it_is_recorded() {
    let dir = std::env::temp_dir().join(format!("tpt-inv-ledger-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("ledger.jsonl");
    let mut ledger = Ledger::load(&path).unwrap();
    let parsed = Parsed::Ok {
        format: DocFormat::Json,
        value: base(),
    };
    let first = check_invoice(&schema(), None, None, Some(&ledger), &parsed);
    assert_eq!(first.verdict, Verdict::Pass);
    ledger.record("123-456-789|T-1").unwrap();
    ledger.record("123-456-789|T-1").unwrap();

    let reloaded = Ledger::load(&path).unwrap();
    let second = check_invoice(&schema(), None, None, Some(&reloaded), &parsed);
    assert_eq!(second.verdict, Verdict::Reject);
    assert_eq!(second.duplicate_key.as_deref(), Some("123-456-789|T-1"));
    assert_eq!(
        std::fs::read_to_string(&path).unwrap().lines().count(),
        1,
        "key written once"
    );
}

#[test]
fn supplier_list_must_be_an_array() {
    assert!(Suppliers::parse("{\"a\": 1}").is_err());
}

// --- CLI ---------------------------------------------------------------------

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_tpt-invoice"))
        .args(args)
        .output()
        .expect("tpt-invoice runs")
}

fn code(o: &Output) -> i32 {
    o.status.code().expect("exits normally")
}

#[test]
fn cli_exit_code_is_the_worst_verdict() {
    let out = run(&[
        "validate",
        example("invoice-ok.json").to_str().unwrap(),
        example("invoice-mismatch.json").to_str().unwrap(),
        "--schema",
        example("invoice.schema.yaml").to_str().unwrap(),
        "--policy",
        example("invoice.policy.yaml").to_str().unwrap(),
        "--suppliers",
        example("suppliers.json").to_str().unwrap(),
    ]);
    assert_eq!(code(&out), 30);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("PASS   "), "{text}");
    assert!(text.contains("REJECT "), "{text}");
}

#[test]
fn cli_ledger_rejects_the_repeat_across_runs() {
    let dir = std::env::temp_dir().join(format!("tpt-inv-cli-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let ledger = dir.join("ledger.jsonl");
    let args = || {
        vec![
            "validate".to_string(),
            example("invoice-ok.json").to_string_lossy().into_owned(),
            "--schema".to_string(),
            example("invoice.schema.yaml")
                .to_string_lossy()
                .into_owned(),
            "--ledger".to_string(),
            ledger.to_string_lossy().into_owned(),
        ]
    };
    let first = args();
    let refs: Vec<&str> = first.iter().map(String::as_str).collect();
    assert_eq!(code(&run(&refs)), 0);
    let second = args();
    let refs: Vec<&str> = second.iter().map(String::as_str).collect();
    assert_eq!(
        code(&run(&refs)),
        30,
        "the same invoice again is a duplicate"
    );
}

#[test]
fn cli_writes_results_per_invoice() {
    let dir = std::env::temp_dir().join(format!("tpt-inv-out-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let out = run(&[
        "validate",
        example("invoice-large.xml").to_str().unwrap(),
        "--schema",
        example("invoice.schema.yaml").to_str().unwrap(),
        "--policy",
        example("invoice.policy.yaml").to_str().unwrap(),
        "--out",
        dir.to_str().unwrap(),
    ]);
    assert_eq!(code(&out), 20);
    let v: Value = serde_json::from_str(
        &std::fs::read_to_string(dir.join("invoice-large.result.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(v["result"]["verdict"], "REVIEW");
}

#[test]
fn cli_bad_supplier_list_exits_2() {
    let dir = std::env::temp_dir().join(format!("tpt-inv-sup-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let bad = dir.join("bad.json");
    std::fs::write(&bad, "{}").unwrap();
    let out = run(&[
        "validate",
        example("invoice-ok.json").to_str().unwrap(),
        "--schema",
        example("invoice.schema.yaml").to_str().unwrap(),
        "--suppliers",
        bad.to_str().unwrap(),
    ]);
    assert_eq!(code(&out), 2);
}

#[test]
fn doctor_passes() {
    assert_eq!(code(&run(&["doctor"])), 0);
}
