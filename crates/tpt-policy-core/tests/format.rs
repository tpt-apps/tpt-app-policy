use tpt_policy_core::{parse_policy, POLICY_FORMAT};

fn with_format(line: &str) -> String {
    format!("policy: purchasing\n{line}\nrules:\n  - id: a\n    when:\n      x: {{ equals: 1 }}\n    then:\n      decision: approved\n")
}

#[test]
fn omitted_format_is_the_engine_format() {
    let p = parse_policy(&with_format("")).unwrap();
    assert_eq!(p.format, POLICY_FORMAT);
    assert_eq!(p.format, 1);
}

#[test]
fn the_engine_format_is_accepted() {
    let p = parse_policy(&with_format("format: 1")).unwrap();
    assert_eq!(p.format, 1);
}

#[test]
fn a_newer_format_is_refused_with_a_fix() {
    let err = parse_policy(&with_format("format: 2")).unwrap_err();
    assert_eq!(err.what, "unsupported format");
    assert!(err.why.contains("format 2"), "{}", err.why);
    assert!(err.fix.contains("upgrade"), "{}", err.fix);
    assert_eq!(err.line, Some(2), "should point at the format line");
}

#[test]
fn zero_is_refused() {
    let err = parse_policy(&with_format("format: 0")).unwrap_err();
    assert_eq!(err.what, "invalid format");
}

#[test]
fn a_decimal_format_is_refused() {
    let err = parse_policy(&with_format("format: 1.5")).unwrap_err();
    assert_eq!(err.what, "invalid format");
}

#[test]
fn a_quoted_format_is_refused() {
    let err = parse_policy(&with_format("format: \"1\"")).unwrap_err();
    assert_eq!(err.what, "wrong type");
}
