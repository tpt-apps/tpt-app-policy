//! Converts policy YAML into the validated [`Policy`] model.
//!
//! Parsing stops at the first problem. Each error says where the problem is,
//! why it is a problem, and how to fix it.

use std::collections::HashSet;

use serde_json::Value as Json;
use serde_yaml::{Mapping, Value as Yaml};

use crate::error::PolicyError;
use crate::locate;
use crate::model::{Check, Condition, Decision, Expectation, Op, Outcome, Policy, Rule, TestCase};

const POLICY_KEYS: &[&str] = &[
    "policy",
    "version",
    "format",
    "default_decision",
    "rules",
    "tests",
];
const RULE_KEYS: &[&str] = &["id", "description", "when", "then"];
const OUTCOME_KEYS: &[&str] = &["decision", "approver", "requirement", "warning"];
const TEST_KEYS: &[&str] = &["name", "input", "expect"];
const EXPECT_KEYS: &[&str] = &["decision", "matched_rules"];

/// Parse and validate a policy from YAML text.
pub fn parse_policy(source: &str) -> Result<Policy, PolicyError> {
    let doc: Yaml = serde_yaml::from_str(source).map_err(|e| {
        let at = match e.location() {
            Some(loc) => format!("line {}, column {}", loc.line(), loc.column()),
            None => "the start of the file".to_string(),
        };
        PolicyError::new(
            "invalid YAML",
            at,
            "the file is not valid YAML",
            "fix the syntax at the location shown; indent with spaces, not tabs",
        )
    })?;
    parse_document(&doc).map_err(|mut e| {
        e.line = locate::line_of(source, &e.location);
        e
    })
}

/// Validate the parsed YAML. Errors carry a key path, which
/// [`parse_policy`] turns into a line number.
fn parse_document(doc: &Yaml) -> Result<Policy, PolicyError> {
    if doc.is_null() {
        return Err(PolicyError::new(
            "empty policy",
            "<root>",
            "the file contains no policy",
            "start the file with 'policy: <name>' and add a rules list",
        ));
    }

    let root = as_map(doc, "<root>")?;
    check_keys(root, POLICY_KEYS, "<root>")?;

    let name = req_text(root, "policy", "<root>")?;
    let version = match get(root, "version") {
        None => "unversioned".to_string(),
        Some(Yaml::Number(n)) if n.is_f64() => {
            return Err(PolicyError::new(
                "unquoted decimal version",
                "version",
                "YAML reads 1.10 as the number 1.1, so the version would be recorded wrongly",
                "quote the version, e.g. version: \"1.10\"",
            ));
        }
        Some(v) => {
            let text = string_of(v, "version")?;
            check_version(&text)?;
            text
        }
    };
    let format = match get(root, "format") {
        None => crate::POLICY_FORMAT,
        Some(v) => check_format(v)?,
    };
    let default_decision = match get(root, "default_decision") {
        None => Decision::Review,
        Some(v) => parse_decision(v, "default_decision")?,
    };

    let rules_src = as_seq(req(root, "rules", "<root>")?, "rules")?;
    if rules_src.is_empty() {
        return Err(PolicyError::new(
            "no rules",
            "rules",
            "the rules list is empty, so every input would get the default decision",
            "add at least one rule with an id, a when clause and a then clause",
        ));
    }
    let mut rules = Vec::with_capacity(rules_src.len());
    let mut ids = HashSet::new();
    for (i, item) in rules_src.iter().enumerate() {
        let loc = format!("rules[{i}]");
        let rule = parse_rule(item, &loc)?;
        if !ids.insert(rule.id.clone()) {
            return Err(PolicyError::new(
                "duplicate rule id",
                format!("{loc}.id"),
                format!("'{}' is already used by an earlier rule", rule.id),
                "give each rule a unique id",
            ));
        }
        rules.push(rule);
    }

    let tests = match get(root, "tests") {
        None => Vec::new(),
        Some(v) => as_seq(v, "tests")?
            .iter()
            .enumerate()
            .map(|(i, t)| parse_test(t, &format!("tests[{i}]")))
            .collect::<Result<Vec<_>, _>>()?,
    };

    Ok(Policy {
        name,
        version,
        format,
        default_decision,
        rules,
        tests,
    })
}

fn parse_rule(value: &Yaml, loc: &str) -> Result<Rule, PolicyError> {
    let map = as_map(value, loc)?;
    check_keys(map, RULE_KEYS, loc)?;
    let id = req_text(map, "id", loc)?;
    let description = opt_string(map, "description", loc)?;
    let when = parse_condition(req(map, "when", loc)?, &format!("{loc}.when"))?;
    let then = parse_outcome(req(map, "then", loc)?, &format!("{loc}.then"))?;
    Ok(Rule {
        id,
        description,
        when,
        then,
    })
}

fn parse_outcome(value: &Yaml, loc: &str) -> Result<Outcome, PolicyError> {
    let map = as_map(value, loc)?;
    check_keys(map, OUTCOME_KEYS, loc)?;
    let decision = parse_decision(req(map, "decision", loc)?, &format!("{loc}.decision"))?;

    let approver = match get(map, "approver") {
        None => None,
        // Accept `approver: manager` and the spec's `approver: {role: manager}`.
        Some(Yaml::Mapping(inner)) => {
            let inner_loc = format!("{loc}.approver");
            check_keys(inner, &["role"], &inner_loc)?;
            Some(req_text(inner, "role", &inner_loc)?)
        }
        Some(v) => Some(string_of(v, &format!("{loc}.approver"))?),
    };

    Ok(Outcome {
        decision,
        approver,
        requirement: opt_string(map, "requirement", loc)?,
        warning: opt_string(map, "warning", loc)?,
    })
}

fn parse_condition(value: &Yaml, loc: &str) -> Result<Condition, PolicyError> {
    let map = as_map(value, loc)?;
    if map.is_empty() {
        return Err(PolicyError::new(
            "empty when clause",
            loc,
            "a condition must check at least one field",
            "add a check such as 'amount: {gt: 1000}'",
        ));
    }

    let mut conditions = Vec::new();
    for (key, val) in map.iter() {
        let key = key.as_str().ok_or_else(|| {
            PolicyError::new(
                "invalid key",
                loc,
                "condition keys must be text",
                "quote the key",
            )
        })?;
        let key_loc = format!("{loc}.{key}");
        match key {
            "all" => conditions.push(Condition::All(parse_condition_list(val, &key_loc)?)),
            "any" => conditions.push(Condition::Any(parse_condition_list(val, &key_loc)?)),
            "not" => conditions.push(Condition::Not(Box::new(parse_condition(val, &key_loc)?))),
            _ => conditions.extend(
                parse_field(key, val, &key_loc)?
                    .into_iter()
                    .map(Condition::Check),
            ),
        }
    }

    if conditions.len() == 1 {
        Ok(conditions.remove(0))
    } else {
        // Sibling keys in one `when` block are combined with AND.
        Ok(Condition::All(conditions))
    }
}

fn parse_condition_list(value: &Yaml, loc: &str) -> Result<Vec<Condition>, PolicyError> {
    let items = as_seq(value, loc)?;
    if items.is_empty() {
        return Err(PolicyError::new(
            "empty condition list",
            loc,
            "all/any need at least one condition",
            "add a condition to the list, or remove the block",
        ));
    }
    items
        .iter()
        .enumerate()
        .map(|(i, item)| parse_condition(item, &format!("{loc}[{i}]")))
        .collect()
}

/// A field key maps to either operators (`{gt: 5}`) or a plain value,
/// which means `equals`.
fn parse_field(path: &str, value: &Yaml, loc: &str) -> Result<Vec<Check>, PolicyError> {
    if path.split('.').any(str::is_empty) {
        return Err(PolicyError::new(
            "invalid field path",
            loc,
            format!("'{path}' has an empty segment"),
            "use dotted names such as expense.amount",
        ));
    }

    match value {
        Yaml::Mapping(ops) => {
            if ops.is_empty() {
                return Err(PolicyError::new(
                    "no operator",
                    loc,
                    format!("field '{path}' has no operator"),
                    "add an operator such as 'gt: 1000' or 'equals: value'",
                ));
            }
            ops.iter()
                .map(|(op_key, op_val)| {
                    let name = op_key.as_str().ok_or_else(|| {
                        PolicyError::new(
                            "invalid operator",
                            loc,
                            "operator names must be text",
                            "quote the operator",
                        )
                    })?;
                    let op = parse_op(name, op_val, &format!("{loc}.{name}"))?;
                    Ok(Check {
                        path: path.to_string(),
                        op,
                    })
                })
                .collect()
        }
        _ => Ok(vec![Check {
            path: path.to_string(),
            op: Op::Equals(to_json(value, loc)?),
        }]),
    }
}

fn parse_op(name: &str, value: &Yaml, loc: &str) -> Result<Op, PolicyError> {
    match name {
        "equals" => Ok(Op::Equals(to_json(value, loc)?)),
        "not_equals" => Ok(Op::NotEquals(to_json(value, loc)?)),
        "contains" => Ok(Op::Contains(to_json(value, loc)?)),
        "gt" | "gte" | "lt" | "lte" => {
            let bound = to_json(value, loc)?;
            if !(bound.is_number() || bound.is_string()) {
                return Err(PolicyError::new(
                    "invalid comparison value",
                    loc,
                    format!("'{name}' needs a number or text, got {bound}"),
                    "compare against a number such as 1000, or against text",
                ));
            }
            Ok(match name {
                "gt" => Op::Gt(bound),
                "gte" => Op::Gte(bound),
                "lt" => Op::Lt(bound),
                _ => Op::Lte(bound),
            })
        }
        "in" | "not_in" => {
            let items = as_seq(value, loc)?
                .iter()
                .map(|item| to_json(item, loc))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(if name == "in" {
                Op::In(items)
            } else {
                Op::NotIn(items)
            })
        }
        "exists" => match value {
            Yaml::Bool(b) => Ok(Op::Exists(*b)),
            _ => Err(PolicyError::new(
                "invalid exists value",
                loc,
                "'exists' takes true or false",
                "write 'exists: true' or 'exists: false'",
            )),
        },
        other => Err(PolicyError::new(
            "unknown operator",
            loc,
            format!("'{other}' is not an operator"),
            "use one of: equals, not_equals, gt, gte, lt, lte, in, not_in, contains, exists",
        )),
    }
}

fn parse_test(value: &Yaml, loc: &str) -> Result<TestCase, PolicyError> {
    let map = as_map(value, loc)?;
    check_keys(map, TEST_KEYS, loc)?;
    let name = req_text(map, "name", loc)?;
    let input = to_json(req(map, "input", loc)?, &format!("{loc}.input"))?;

    let expect_loc = format!("{loc}.expect");
    let expect_map = as_map(req(map, "expect", loc)?, &expect_loc)?;
    check_keys(expect_map, EXPECT_KEYS, &expect_loc)?;
    let decision = parse_decision(
        req(expect_map, "decision", &expect_loc)?,
        &format!("{expect_loc}.decision"),
    )?;
    let matched_rules = match get(expect_map, "matched_rules") {
        None => None,
        Some(v) => {
            let list_loc = format!("{expect_loc}.matched_rules");
            Some(
                as_seq(v, &list_loc)?
                    .iter()
                    .map(|item| string_of(item, &list_loc))
                    .collect::<Result<Vec<_>, _>>()?,
            )
        }
    };

    Ok(TestCase {
        name,
        input,
        expect: Expectation {
            decision,
            matched_rules,
        },
    })
}

fn parse_decision(value: &Yaml, loc: &str) -> Result<Decision, PolicyError> {
    let text = string_of(value, loc)?;
    Decision::parse(&text).ok_or_else(|| {
        PolicyError::new(
            "unknown decision",
            loc,
            format!("'{text}' is not a decision"),
            "use one of: approved, review, approval_required, rejected",
        )
    })
}

fn to_json(value: &Yaml, loc: &str) -> Result<Json, PolicyError> {
    serde_json::to_value(value).map_err(|e| {
        PolicyError::new(
            "invalid value",
            loc,
            e.to_string(),
            "use a plain number, text, true/false, or a list",
        )
    })
}

fn as_map<'a>(value: &'a Yaml, loc: &str) -> Result<&'a Mapping, PolicyError> {
    value.as_mapping().ok_or_else(|| {
        PolicyError::new(
            "wrong type",
            loc,
            "expected a set of key: value pairs",
            "indent the keys under this item",
        )
    })
}

fn as_seq<'a>(value: &'a Yaml, loc: &str) -> Result<&'a Vec<Yaml>, PolicyError> {
    value.as_sequence().ok_or_else(|| {
        PolicyError::new(
            "wrong type",
            loc,
            "expected a list",
            "write each item on its own line starting with '- '",
        )
    })
}

fn get<'a>(map: &'a Mapping, key: &str) -> Option<&'a Yaml> {
    map.get(key)
}

fn req<'a>(map: &'a Mapping, key: &str, loc: &str) -> Result<&'a Yaml, PolicyError> {
    get(map, key).ok_or_else(|| {
        PolicyError::new(
            "missing key",
            loc,
            format!("'{key}' is required here"),
            format!("add '{key}:' under {loc}"),
        )
    })
}

/// A required, non-empty text value.
fn req_text(map: &Mapping, key: &str, loc: &str) -> Result<String, PolicyError> {
    let text = string_of(req(map, key, loc)?, &format!("{loc}.{key}"))?;
    if text.trim().is_empty() {
        return Err(PolicyError::new(
            "empty value",
            format!("{loc}.{key}"),
            format!("'{key}' cannot be blank"),
            format!("give '{key}' a value"),
        ));
    }
    Ok(text)
}

fn opt_string(map: &Mapping, key: &str, loc: &str) -> Result<Option<String>, PolicyError> {
    get(map, key)
        .map(|v| string_of(v, &format!("{loc}.{key}")))
        .transpose()
}

/// The policy format must be a whole number from 1 up to the engine's format.
fn check_format(value: &Yaml) -> Result<u32, PolicyError> {
    let Yaml::Number(n) = value else {
        return Err(PolicyError::new(
            "wrong type",
            "format",
            "the policy format must be a whole number",
            "write it without quotes, e.g. format: 1",
        ));
    };
    let Some(format) = n.as_u64() else {
        return Err(PolicyError::new(
            "invalid format",
            "format",
            format!("'{n}' is not a whole number"),
            "write format: 1",
        ));
    };
    if format == 0 {
        return Err(PolicyError::new(
            "invalid format",
            "format",
            "format 0 does not exist",
            "write format: 1",
        ));
    }
    if format > u64::from(crate::POLICY_FORMAT) {
        return Err(PolicyError::new(
            "unsupported format",
            "format",
            format!(
                "this policy declares format {format}, but this engine reads format {} only",
                crate::POLICY_FORMAT
            ),
            "upgrade tpt-policy to a version that reads this format, or change the policy to format 1",
        ));
    }
    Ok(format as u32)
}

/// A policy version is `MAJOR`, `MAJOR.MINOR` or `MAJOR.MINOR.PATCH`, with
/// numeric parts only, e.g. `1`, `2.1` or `2.1.0`.
fn check_version(text: &str) -> Result<(), PolicyError> {
    let parts: Vec<&str> = text.split('.').collect();
    let valid = (1..=3).contains(&parts.len())
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()));
    if valid {
        return Ok(());
    }
    Err(PolicyError::new(
        "invalid version",
        "version",
        format!("'{text}' is not a version number"),
        "use MAJOR.MINOR.PATCH with numbers only, e.g. version: \"2.1.0\", or leave version out",
    ))
}

fn string_of(value: &Yaml, loc: &str) -> Result<String, PolicyError> {
    match value {
        Yaml::String(s) => Ok(s.clone()),
        Yaml::Number(n) => Ok(n.to_string()),
        _ => Err(PolicyError::new(
            "wrong type",
            loc,
            "expected text",
            "wrap the value in quotes",
        )),
    }
}

fn check_keys(map: &Mapping, allowed: &[&str], loc: &str) -> Result<(), PolicyError> {
    for key in map.keys() {
        let name = key.as_str().unwrap_or("<non-text key>");
        if !allowed.contains(&name) {
            return Err(PolicyError::new(
                "unknown key",
                loc,
                format!("'{name}' is not a recognised key here"),
                format!("use one of: {}", allowed.join(", ")),
            ));
        }
    }
    Ok(())
}
