use tpt_policy_core::{evaluate, parse_policy};

fn with_version(line: &str) -> String {
    format!("policy: purchasing\n{line}\nrules:\n  - id: a\n    when:\n      x: {{ equals: 1 }}\n    then:\n      decision: approved\n")
}

#[test]
fn omitted_version_is_unversioned() {
    let p = parse_policy("policy: purchasing\nrules:\n  - id: a\n    when:\n      x: { equals: 1 }\n    then:\n      decision: approved\n").unwrap();
    assert_eq!(p.version, "unversioned");
}

#[test]
fn numeric_versions_are_accepted() {
    for v in [
        "version: 1",
        "version: \"2.1\"",
        "version: \"2.1.0\"",
        "version: 1.0.0",
    ] {
        assert!(
            parse_policy(&with_version(v)).is_ok(),
            "{v} should be accepted"
        );
    }
}

#[test]
fn invalid_versions_are_rejected_with_location() {
    for v in [
        "version: \"v2.1.0\"",
        "version: \"2.1.0-beta\"",
        "version: \"1.2.3.4\"",
        "version: \"1..2\"",
        "version: \"\"",
    ] {
        let e = parse_policy(&with_version(v)).expect_err(v);
        assert_eq!(e.what, "invalid version", "{v}");
        assert_eq!(e.location, "version", "{v}");
    }
}

#[test]
fn unquoted_decimal_version_is_rejected() {
    let e = parse_policy(&with_version("version: 1.10")).expect_err("1.10 is a float in YAML");
    assert_eq!(e.what, "unquoted decimal version");
}

#[test]
fn policy_id_combines_name_and_version() {
    let p = parse_policy(&with_version("version: \"2.1.0\"")).unwrap();
    let input = serde_json::json!({ "x": 1 });
    assert_eq!(evaluate(&p, &input).policy.id(), "purchasing@2.1.0");
}
