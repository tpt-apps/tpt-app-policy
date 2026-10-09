use serde_json::json;
use tpt_schema::{check_record, lookup, parse_schema, Mode, UniqueIndex};

const SCHEMA: &str = r#"
schema: customer
version: "1.0.0"
fields:
  customer_id:
    type: string
    required: true
    unique: true
  email:
    type: string
    required: true
    pattern: '^[^@\s]+@[^@\s]+\.[^@\s]+$'
  country:
    type: string
    enum: [NZ, AU]
  age:
    type: integer
    min: 0
    max: 150
  score:
    type: number
  active:
    type: boolean
  address.postcode:
    type: string
"#;

fn schema() -> tpt_schema::Schema {
    parse_schema(SCHEMA).expect("test schema is valid")
}

fn reasons(record: serde_json::Value, mode: Mode) -> Vec<String> {
    check_record(&schema(), &record, mode)
        .into_iter()
        .map(|e| format!("{}: {}", e.field, e.reason))
        .collect()
}

#[test]
fn parses_name_version_and_fields_in_order() {
    let s = schema();
    assert_eq!(s.name, "customer");
    assert_eq!(s.version, "1.0.0");
    let paths: Vec<&str> = s.fields.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(
        paths,
        [
            "customer_id",
            "email",
            "country",
            "age",
            "score",
            "active",
            "address.postcode"
        ]
    );
}

#[test]
fn version_defaults_to_unversioned() {
    let s = parse_schema("schema: x\nfields:\n  a:\n    type: string\n").unwrap();
    assert_eq!(s.version, "unversioned");
}

#[test]
fn valid_record_has_no_errors() {
    let record = json!({
        "customer_id": "C1",
        "email": "a@b.co",
        "country": "NZ",
        "age": 30,
        "score": 1.5,
        "active": true,
        "address": {"postcode": "6011"}
    });
    assert!(reasons(record, Mode::Strict).is_empty());
}

#[test]
fn required_missing_and_null_both_fail() {
    let r = reasons(json!({"customer_id": null}), Mode::Strict);
    assert!(r.contains(&"customer_id: is required".to_string()), "{r:?}");
    assert!(r.contains(&"email: is required".to_string()), "{r:?}");
}

#[test]
fn enum_pattern_and_range_are_checked() {
    let r = reasons(
        json!({"customer_id": "C1", "email": "nope", "country": "FR", "age": 200}),
        Mode::Strict,
    );
    assert!(
        r.iter()
            .any(|e| e.starts_with("email: 'nope' does not match")),
        "{r:?}"
    );
    assert!(
        r.iter()
            .any(|e| e.starts_with("country: 'FR' is not one of")),
        "{r:?}"
    );
    assert!(
        r.iter().any(|e| e == "age: 200 is above the maximum 150"),
        "{r:?}"
    );
}

#[test]
fn strict_mode_rejects_text_for_numbers() {
    let r = reasons(
        json!({"customer_id": "C1", "email": "a@b.co", "age": "30"}),
        Mode::Strict,
    );
    assert_eq!(r, vec!["age: expected an integer, got '30'".to_string()]);
}

#[test]
fn text_mode_reads_numbers_and_booleans_from_text() {
    let r = reasons(
        json!({"customer_id": "C1", "email": "a@b.co", "age": "30", "score": "2.5", "active": "TRUE"}),
        Mode::Text,
    );
    assert!(r.is_empty(), "{r:?}");
}

#[test]
fn text_mode_rejects_non_finite_and_fractional_integers() {
    let r = reasons(
        json!({"customer_id": "C1", "email": "a@b.co", "score": "NaN", "age": "3.5"}),
        Mode::Text,
    );
    assert!(
        r.contains(&"score: expected a number, got 'NaN'".to_string()),
        "{r:?}"
    );
    assert!(
        r.contains(&"age: expected an integer, got '3.5'".to_string()),
        "{r:?}"
    );
}

#[test]
fn nested_paths_are_looked_up() {
    let record = json!({"address": {"postcode": 6011}});
    assert_eq!(lookup(&record, "address.postcode"), Some(&json!(6011)));
    let list = json!({"items": [{"sku": "A"}]});
    assert_eq!(lookup(&list, "items.0.sku"), Some(&json!("A")));
    assert_eq!(lookup(&list, "items.5.sku"), None);
}

#[test]
fn unique_index_reports_first_row_of_duplicate() {
    let s = schema();
    let mut index = UniqueIndex::new();
    assert!(index.check(&s, &json!({"customer_id": "C1"}), 1).is_empty());
    let dup = index.check(&s, &json!({"customer_id": "C1"}), 7);
    assert_eq!(dup.len(), 1);
    assert_eq!(dup[0].reason, "duplicate value 'C1' (first seen at row 1)");
}

#[test]
fn rejects_bad_schemas_with_location() {
    let cases: &[(&str, &str)] = &[
        ("schema: x\nfields:\n  a:\n    type: date\n", "unknown type"),
        (
            "schema: x\nfields:\n  a:\n    type: integer\n    pattern: '^\\d+$'\n",
            "pattern on a non-text field",
        ),
        (
            "schema: x\nfields:\n  a:\n    type: string\n    pattern: '('\n",
            "invalid pattern",
        ),
        (
            "schema: x\nfields:\n  a:\n    type: integer\n    min: 9\n    max: 1\n",
            "min is greater than max",
        ),
        (
            "schema: x\nfields:\n  a:\n    type: string\n    enum: []\n",
            "empty enum",
        ),
        (
            "schema: x\nfields:\n  a:\n    type: string\n    enum: [[1]]\n",
            "invalid enum value",
        ),
        ("schema: x\nfields: {}\n", "no fields"),
        (
            "schema: x\nfields:\n  a.:\n    type: string\n",
            "invalid field name",
        ),
        (
            "schema: x\nfields:\n  a:\n    type: string\n    colour: red\n",
            "invalid field definition",
        ),
        ("schema: [unclosed\n", "invalid schema YAML"),
    ];
    for (yaml, what) in cases {
        let err = parse_schema(yaml).expect_err(yaml);
        assert_eq!(err.what, *what, "for schema:\n{yaml}");
        assert!(!err.fix.is_empty());
    }
}
