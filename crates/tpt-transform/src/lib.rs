//! Pipeline engine for TPT Data Transformer.
//!
//! A pipeline is a YAML list of steps. Each record passes through the steps in
//! order. A step that cannot process a record rejects it, with the reason. A
//! filter step drops records that do not match, without rejecting them.
//!
//! Field names are dotted paths, as in the data validator: `address.city` is the
//! `city` key inside `address`. The name `*` means every text value in the
//! record, and is allowed only in the trim and case steps.

use std::collections::BTreeSet;

use serde::Deserialize;
use serde_json::{Map, Number, Value};
use tpt_data_core::Item;

/// Version of this crate, recorded in summaries.
pub const TRANSFORM_VERSION: &str = env!("CARGO_PKG_VERSION");

/// A parsed pipeline file.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PipelineFile {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    pub pipeline: Vec<Step>,
}

/// One step. In YAML, each step is a single key naming the step, for example
/// `- trim: { fields: ["*"] }`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Step {
    /// Remove spaces at both ends of text values.
    Trim { fields: Vec<String> },
    /// Lower case text values.
    Lowercase { fields: Vec<String> },
    /// Upper case text values.
    Uppercase { fields: Vec<String> },
    /// Capitalise each word of text values, and lower case the rest of each word.
    TitleCase { fields: Vec<String> },
    /// Move values to new names. `old: new` pairs. A missing field is skipped.
    Rename(std::collections::BTreeMap<String, String>),
    /// Remove fields. A missing field is skipped.
    Delete { fields: Vec<String> },
    /// Keep only these fields. Everything else is removed.
    Select { fields: Vec<String> },
    /// Convert text to a number. Fails on text that is not a number.
    ToNumber { fields: Vec<String> },
    /// Convert numbers and booleans to text.
    ToText { fields: Vec<String> },
    /// Convert yes/no, true/false, 1/0 text to a boolean.
    ToBool { fields: Vec<String> },
    /// Set a value when the field is missing, null or empty text.
    Defaults(std::collections::BTreeMap<String, Value>),
    /// Set values when a condition holds.
    Conditional {
        when: Condition,
        set: std::collections::BTreeMap<String, Value>,
    },
    /// Join text values into one new field.
    Combine {
        fields: Vec<String>,
        into: String,
        #[serde(default = "space")]
        separator: String,
        /// Keep the source fields. By default they are removed.
        #[serde(default)]
        keep_sources: bool,
    },
    /// Split one text value into several new fields. Parts are trimmed, and
    /// empty parts become null. More parts than fields is an error. The source
    /// field is removed, unless it is one of the new fields.
    Split {
        field: String,
        into: Vec<String>,
        separator: String,
    },
    /// Replace a text value with a value from a table. Values not in the table
    /// are left as they are.
    Map {
        field: String,
        values: std::collections::BTreeMap<String, Value>,
    },
    /// Keep only records that match. Records that do not match are dropped and
    /// counted, but not rejected.
    Filter(Condition),
    /// Arithmetic on numeric fields, into a new field.
    Calculate {
        into: String,
        operation: Operation,
        fields: Vec<String>,
        /// Round the result to this many decimal places (at most 10).
        #[serde(default)]
        decimals: Option<u32>,
    },
}

/// The arithmetic in a `calculate` step.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Operation {
    Add,
    Subtract,
    Multiply,
    Divide,
}

/// A test on one field. Exactly one of `equals`, `not_equals` or `one_of`.
/// Values are compared as text, so the number 0 matches the text "0".
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Condition {
    pub field: String,
    #[serde(default)]
    pub equals: Option<Value>,
    #[serde(default)]
    pub not_equals: Option<Value>,
    #[serde(default)]
    pub one_of: Option<Vec<Value>>,
}

fn space() -> String {
    " ".to_string()
}

/// What happened to one record.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// The record after every step.
    Keep(Value),
    /// A filter step did not match.
    Drop,
}

/// A record that could not be transformed.
#[derive(Debug, Clone, PartialEq)]
pub struct Rejection {
    pub number: u64,
    pub reason: String,
    /// The record as it was read.
    pub record: Value,
}

/// The result of running a pipeline over a whole input.
#[derive(Debug, Default)]
pub struct Run {
    pub read: u64,
    /// `(record number, transformed record)`, in input order.
    pub kept: Vec<(u64, Value)>,
    pub filtered: u64,
    pub rejected: Vec<Rejection>,
}

/// Parse and check a pipeline file. Errors name the step and say what is wrong.
///
/// The YAML is read into a generic value and then into the typed pipeline. This
/// lets each step be written as a single key, such as `- trim: {...}`, which
/// serde_yaml does not accept for enums on its own.
pub fn parse_pipeline(text: &str) -> Result<PipelineFile, String> {
    let yaml: serde_yaml::Value = serde_yaml::from_str(text).map_err(|e| e.to_string())?;
    let json = serde_json::to_value(&yaml).map_err(|e| e.to_string())?;
    let file: PipelineFile = serde_json::from_value(json).map_err(|e| e.to_string())?;
    validate(&file)?;
    Ok(file)
}

fn validate(file: &PipelineFile) -> Result<(), String> {
    if file.pipeline.is_empty() {
        return Err("the pipeline has no steps; add at least one step under 'pipeline:'".into());
    }
    for (index, step) in file.pipeline.iter().enumerate() {
        let n = index + 1;
        let name = step.name();
        let fail = |why: &str| Err(format!("step {n} ({name}): {why}"));
        match step {
            Step::Trim { fields }
            | Step::Lowercase { fields }
            | Step::Uppercase { fields }
            | Step::TitleCase { fields } => {
                check_fields(n, name, fields, true)?;
            }
            Step::Delete { fields }
            | Step::Select { fields }
            | Step::ToNumber { fields }
            | Step::ToText { fields }
            | Step::ToBool { fields } => check_fields(n, name, fields, false)?,
            Step::Rename(pairs) => {
                if pairs.is_empty() {
                    return fail("no pairs given; use 'old_name: new_name'");
                }
                if pairs
                    .iter()
                    .any(|(old, new)| old.is_empty() || new.is_empty())
                {
                    return fail("field names cannot be empty");
                }
            }
            Step::Defaults(values) => {
                if values.is_empty() {
                    return fail("no fields given; use 'field: value'");
                }
            }
            Step::Conditional { when, set } => {
                check_condition(n, name, when)?;
                if set.is_empty() {
                    return fail("'set' is empty; list at least one field to set");
                }
            }
            Step::Filter(condition) => check_condition(n, name, condition)?,
            Step::Combine { fields, into, .. } => {
                check_fields(n, name, fields, false)?;
                if into.is_empty() {
                    return fail("'into' is empty; name the new field");
                }
            }
            Step::Split {
                field,
                into,
                separator,
            } => {
                if field.is_empty() || into.is_empty() || into.iter().any(String::is_empty) {
                    return fail("'field' and every 'into' name must be set");
                }
                if separator.is_empty() {
                    return fail("'separator' cannot be empty");
                }
            }
            Step::Map { field, values } => {
                if field.is_empty() || values.is_empty() {
                    return fail("'field' and at least one entry in 'values' are needed");
                }
            }
            Step::Calculate {
                into,
                operation,
                fields,
                decimals,
            } => {
                check_fields(n, name, fields, false)?;
                if into.is_empty() {
                    return fail("'into' is empty; name the new field");
                }
                if matches!(operation, Operation::Subtract | Operation::Divide) && fields.len() != 2
                {
                    return fail("subtract and divide need exactly two fields");
                }
                if decimals.is_some_and(|d| d > 10) {
                    return fail("'decimals' must be 10 or less");
                }
            }
        }
    }
    Ok(())
}

fn check_fields(n: usize, name: &str, fields: &[String], allow_all: bool) -> Result<(), String> {
    if fields.is_empty() {
        return Err(format!(
            "step {n} ({name}): 'fields' is empty; list at least one field"
        ));
    }
    if !allow_all && fields.iter().any(|f| f == "*") {
        return Err(format!(
            "step {n} ({name}): '*' is only allowed in trim and case steps; list the fields by name"
        ));
    }
    if fields.iter().any(|f| f.trim().is_empty()) {
        return Err(format!("step {n} ({name}): field names cannot be empty"));
    }
    Ok(())
}

fn check_condition(n: usize, name: &str, condition: &Condition) -> Result<(), String> {
    let given = [
        condition.equals.is_some(),
        condition.not_equals.is_some(),
        condition.one_of.is_some(),
    ]
    .iter()
    .filter(|&&given| given)
    .count();
    if condition.field.is_empty() {
        return Err(format!("step {n} ({name}): the condition needs a 'field'"));
    }
    if given != 1 {
        return Err(format!(
            "step {n} ({name}): the condition needs exactly one of 'equals', 'not_equals' or 'one_of'"
        ));
    }
    Ok(())
}

impl Step {
    /// The step's name in YAML, for messages.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Trim { .. } => "trim",
            Self::Lowercase { .. } => "lowercase",
            Self::Uppercase { .. } => "uppercase",
            Self::TitleCase { .. } => "title_case",
            Self::Rename(_) => "rename",
            Self::Delete { .. } => "delete",
            Self::Select { .. } => "select",
            Self::ToNumber { .. } => "to_number",
            Self::ToText { .. } => "to_text",
            Self::ToBool { .. } => "to_bool",
            Self::Defaults(_) => "defaults",
            Self::Conditional { .. } => "conditional",
            Self::Combine { .. } => "combine",
            Self::Split { .. } => "split",
            Self::Map { .. } => "map",
            Self::Filter(_) => "filter",
            Self::Calculate { .. } => "calculate",
        }
    }
}

impl PipelineFile {
    /// Run every step over one record. `Err` carries the step and the reason.
    pub fn apply(&self, record: &Value) -> Result<Outcome, String> {
        let mut current = record.clone();
        for (index, step) in self.pipeline.iter().enumerate() {
            let root = current
                .as_object_mut()
                .ok_or_else(|| "the record is not a JSON object".to_string())?;
            let keep = step
                .apply(root)
                .map_err(|why| format!("step {} ({}): {why}", index + 1, step.name()))?;
            if !keep {
                return Ok(Outcome::Drop);
            }
        }
        Ok(Outcome::Keep(current))
    }
}

/// Run a pipeline over every item of an input. Items that cannot be read, and
/// records a step rejects, are collected as rejections. The run does not stop.
pub fn run(pipeline: &PipelineFile, items: impl Iterator<Item = Item>) -> Run {
    let mut run = Run::default();
    for item in items {
        run.read += 1;
        match item {
            Item::Row(row) => match pipeline.apply(&row.record) {
                Ok(Outcome::Keep(record)) => run.kept.push((row.number, record)),
                Ok(Outcome::Drop) => run.filtered += 1,
                Err(reason) => run.rejected.push(Rejection {
                    number: row.number,
                    reason,
                    record: row.record,
                }),
            },
            Item::Malformed { number, message } => run.rejected.push(Rejection {
                number,
                reason: message,
                record: Value::Null,
            }),
        }
    }
    run
}

/// Column names for CSV output. Input columns that survive the pipeline keep
/// their order. New fields follow, in the order they first appear.
pub fn output_headers(input: Option<&[String]>, kept: &[(u64, Value)]) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut order = Vec::new();
    for (_, record) in kept {
        let mut leaves = Vec::new();
        leaf_paths(record, "", &mut leaves);
        for leaf in leaves {
            if seen.insert(leaf.clone()) {
                order.push(leaf);
            }
        }
    }
    let mut headers: Vec<String> = input
        .unwrap_or(&[])
        .iter()
        .filter(|h| seen.contains(*h))
        .cloned()
        .collect();
    for leaf in order {
        if !headers.contains(&leaf) {
            headers.push(leaf);
        }
    }
    headers
}

fn leaf_paths(value: &Value, prefix: &str, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let path = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                leaf_paths(child, &path, out);
            }
        }
        _ => out.push(prefix.to_string()),
    }
}

// --- step implementations ---------------------------------------------------

impl Step {
    /// Apply this step to the record. `Ok(false)` means the record is dropped.
    fn apply(&self, root: &mut Map<String, Value>) -> Result<bool, String> {
        match self {
            Self::Trim { fields } => transform_text(root, fields, |s| s.trim().to_string()),
            Self::Lowercase { fields } => transform_text(root, fields, |s| s.to_lowercase()),
            Self::Uppercase { fields } => transform_text(root, fields, |s| s.to_uppercase()),
            Self::TitleCase { fields } => transform_text(root, fields, title_case),
            Self::Rename(pairs) => {
                for (old, new) in pairs {
                    if let Some(value) = remove_path(root, old) {
                        put_path(root, new, value);
                    }
                }
            }
            Self::Delete { fields } => {
                for field in fields {
                    remove_path(root, field);
                }
            }
            Self::Select { fields } => {
                let mut kept = Map::new();
                for field in fields {
                    if let Some(value) = get_path(root, field) {
                        put_path(&mut kept, field, value.clone());
                    }
                }
                *root = kept;
            }
            Self::ToNumber { fields } => convert(root, fields, to_number)?,
            Self::ToText { fields } => convert(root, fields, |v| match v {
                Value::Number(_) | Value::Bool(_) => Ok(Value::String(v.to_string())),
                other => Ok(other.clone()),
            })?,
            Self::ToBool { fields } => convert(root, fields, to_bool)?,
            Self::Defaults(values) => {
                for (field, value) in values {
                    let empty = match get_path(root, field) {
                        None | Some(Value::Null) => true,
                        Some(Value::String(text)) => text.is_empty(),
                        Some(_) => false,
                    };
                    if empty {
                        put_path(root, field, value.clone());
                    }
                }
            }
            Self::Conditional { when, set } => {
                if when.matches(root) {
                    for (field, value) in set {
                        put_path(root, field, value.clone());
                    }
                }
            }
            Self::Combine {
                fields,
                into,
                separator,
                keep_sources,
            } => {
                let mut parts = Vec::new();
                for field in fields {
                    let text = match get_path(root, field) {
                        None | Some(Value::Null) => String::new(),
                        Some(value) => {
                            text_of(value).map_err(|why| format!("field '{field}': {why}"))?
                        }
                    };
                    parts.push(text);
                }
                if !keep_sources {
                    for field in fields {
                        if field != into {
                            remove_path(root, field);
                        }
                    }
                }
                put_path(root, into, Value::String(parts.join(separator)));
            }
            Self::Split {
                field,
                into,
                separator,
            } => {
                let text = match get_path(root, field) {
                    None | Some(Value::Null) => None,
                    Some(value) => {
                        Some(text_of(value).map_err(|why| format!("field '{field}': {why}"))?)
                    }
                };
                let parts: Vec<&str> = match &text {
                    Some(text) => text.split(separator.as_str()).collect(),
                    None => Vec::new(),
                };
                if parts.len() > into.len() {
                    return Err(format!(
                        "field '{field}' has {} parts, but only {} fields are named in 'into'",
                        parts.len(),
                        into.len()
                    ));
                }
                if !into.contains(field) {
                    remove_path(root, field);
                }
                for (index, name) in into.iter().enumerate() {
                    let value = match parts.get(index).map(|p| p.trim()) {
                        Some(part) if !part.is_empty() => Value::String(part.to_string()),
                        _ => Value::Null,
                    };
                    put_path(root, name, value);
                }
            }
            Self::Map { field, values } => {
                if let Some(value) = get_mut_path(root, field) {
                    if let Ok(key) = text_of(value) {
                        if let Some(mapped) = values.get(&key) {
                            *value = mapped.clone();
                        }
                    }
                }
            }
            Self::Filter(condition) => return Ok(condition.matches(root)),
            Self::Calculate {
                into,
                operation,
                fields,
                decimals,
            } => {
                let mut operands = Vec::new();
                for field in fields {
                    operands.push(number_field(root, field)?);
                }
                let result = calculate(*operation, &operands)?;
                let value = number_value(result, *decimals)?;
                put_path(root, into, value);
            }
        }
        Ok(true)
    }
}

impl Condition {
    fn matches(&self, root: &Map<String, Value>) -> bool {
        let actual = match get_path(root, &self.field) {
            None | Some(Value::Null) => String::new(),
            Some(Value::String(text)) => text.clone(),
            Some(other) => other.to_string(),
        };
        let same = |expected: &Value| as_text(expected) == actual;
        if let Some(expected) = &self.equals {
            same(expected)
        } else if let Some(expected) = &self.not_equals {
            !same(expected)
        } else if let Some(options) = &self.one_of {
            options.iter().any(same)
        } else {
            false
        }
    }
}

/// Text form of a value, as used in comparisons. Null is empty text.
fn as_text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

/// Text of a scalar value. Objects and lists cannot be used as text.
fn text_of(value: &Value) -> Result<String, String> {
    match value {
        Value::Null => Ok(String::new()),
        Value::String(text) => Ok(text.clone()),
        Value::Number(_) | Value::Bool(_) => Ok(value.to_string()),
        _ => Err("an object or list cannot be used as text".to_string()),
    }
}

/// Apply a text function to the named fields. `*` means every text value.
fn transform_text(
    root: &mut Map<String, Value>,
    fields: &[String],
    f: impl Fn(&str) -> String + Copy,
) {
    if fields.iter().any(|field| field == "*") {
        for value in root.values_mut() {
            map_strings(value, f);
        }
    } else {
        for field in fields {
            if let Some(value) = get_mut_path(root, field) {
                map_strings(value, f);
            }
        }
    }
}

fn map_strings(value: &mut Value, f: impl Fn(&str) -> String + Copy) {
    match value {
        Value::String(text) => *text = f(text),
        Value::Object(map) => {
            for child in map.values_mut() {
                map_strings(child, f);
            }
        }
        Value::Array(items) => {
            for child in items {
                map_strings(child, f);
            }
        }
        _ => {}
    }
}

fn title_case(text: &str) -> String {
    text.split(' ')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => {
                    first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase()
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Convert each named field with `f`. A missing field is skipped. Errors name the field.
fn convert(
    root: &mut Map<String, Value>,
    fields: &[String],
    f: impl Fn(&Value) -> Result<Value, String>,
) -> Result<(), String> {
    for field in fields {
        if let Some(value) = get_mut_path(root, field) {
            let converted = f(value).map_err(|why| format!("field '{field}': {why}"))?;
            *value = converted;
        }
    }
    Ok(())
}

fn to_number(value: &Value) -> Result<Value, String> {
    match value {
        Value::Null | Value::Number(_) => Ok(value.clone()),
        Value::String(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return Ok(Value::Null);
            }
            if let Ok(whole) = trimmed.parse::<i64>() {
                return Ok(Value::Number(whole.into()));
            }
            trimmed
                .parse::<f64>()
                .ok()
                .filter(|x| x.is_finite())
                .and_then(Number::from_f64)
                .map(Value::Number)
                .ok_or_else(|| format!("'{text}' is not a number"))
        }
        _ => Err("an object or list cannot be read as a number".to_string()),
    }
}

fn to_bool(value: &Value) -> Result<Value, String> {
    match value {
        Value::Null | Value::Bool(_) => Ok(value.clone()),
        Value::String(text) => match text.trim().to_ascii_lowercase().as_str() {
            "" => Ok(Value::Null),
            "true" | "yes" | "y" | "1" => Ok(Value::Bool(true)),
            "false" | "no" | "n" | "0" => Ok(Value::Bool(false)),
            _ => Err(format!("'{text}' is not yes/no, true/false or 1/0")),
        },
        Value::Number(n) if n.as_i64() == Some(1) => Ok(Value::Bool(true)),
        Value::Number(n) if n.as_i64() == Some(0) => Ok(Value::Bool(false)),
        _ => Err(format!("'{value}' is not yes/no, true/false or 1/0")),
    }
}

fn number_field(root: &Map<String, Value>, field: &str) -> Result<f64, String> {
    match get_path(root, field) {
        None | Some(Value::Null) => Err(format!("field '{field}' is empty")),
        Some(Value::Number(n)) => n
            .as_f64()
            .ok_or_else(|| format!("field '{field}' is not a number")),
        Some(_) => Err(format!(
            "field '{field}' is not a number; add a to_number step before this one"
        )),
    }
}

fn calculate(operation: Operation, operands: &[f64]) -> Result<f64, String> {
    match operation {
        Operation::Add => Ok(operands.iter().sum()),
        Operation::Multiply => Ok(operands.iter().product()),
        Operation::Subtract => Ok(operands[0] - operands[1]),
        Operation::Divide => {
            if operands[1] == 0.0 {
                Err("division by zero".to_string())
            } else {
                Ok(operands[0] / operands[1])
            }
        }
    }
}

fn number_value(x: f64, decimals: Option<u32>) -> Result<Value, String> {
    let scaled = match decimals {
        Some(places) => {
            let factor = 10f64.powi(places as i32);
            (x * factor).round() / factor
        }
        None => x,
    };
    if !scaled.is_finite() {
        return Err("the result is not a finite number".to_string());
    }
    if scaled.fract() == 0.0 && scaled.abs() < 9.0e15 {
        return Ok(Value::Number((scaled as i64).into()));
    }
    Number::from_f64(scaled)
        .map(Value::Number)
        .ok_or_else(|| "the result is not a finite number".to_string())
}

// --- dotted paths -------------------------------------------------------------

fn get_path<'a>(map: &'a Map<String, Value>, path: &str) -> Option<&'a Value> {
    match path.split_once('.') {
        None => map.get(path),
        Some((head, rest)) => match map.get(head)? {
            Value::Object(inner) => get_path(inner, rest),
            _ => None,
        },
    }
}

fn get_mut_path<'a>(map: &'a mut Map<String, Value>, path: &str) -> Option<&'a mut Value> {
    match path.split_once('.') {
        None => map.get_mut(path),
        Some((head, rest)) => match map.get_mut(head)? {
            Value::Object(inner) => get_mut_path(inner, rest),
            _ => None,
        },
    }
}

fn remove_path(map: &mut Map<String, Value>, path: &str) -> Option<Value> {
    match path.split_once('.') {
        None => map.remove(path),
        Some((head, rest)) => match map.get_mut(head)? {
            Value::Object(inner) => remove_path(inner, rest),
            _ => None,
        },
    }
}

fn put_path(map: &mut Map<String, Value>, path: &str, value: Value) {
    match path.split_once('.') {
        None => {
            map.insert(path.to_string(), value);
        }
        Some((head, rest)) => {
            let entry = map
                .entry(head.to_string())
                .or_insert_with(|| Value::Object(Map::new()));
            if !entry.is_object() {
                *entry = Value::Object(Map::new());
            }
            if let Value::Object(inner) = entry {
                put_path(inner, rest, value);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn pipeline(steps: &str) -> PipelineFile {
        parse_pipeline(&format!("pipeline:\n{steps}")).expect("pipeline parses")
    }

    fn run_one(file: &PipelineFile, record: Value) -> Result<Outcome, String> {
        file.apply(&record)
    }

    #[test]
    fn empty_pipeline_is_refused() {
        let err = parse_pipeline("pipeline: []").unwrap_err();
        assert!(err.contains("no steps"), "{err}");
    }

    #[test]
    fn wildcard_is_refused_outside_text_steps() {
        let err = parse_pipeline("pipeline:\n  - delete:\n      fields: [\"*\"]\n").unwrap_err();
        assert!(err.contains("only allowed in trim and case steps"), "{err}");
    }

    #[test]
    fn unknown_step_is_refused() {
        assert!(parse_pipeline("pipeline:\n  - shout:\n      fields: [a]\n").is_err());
    }

    #[test]
    fn condition_needs_exactly_one_operator() {
        let err = parse_pipeline(
            "pipeline:\n  - filter:\n      field: a\n      equals: 1\n      not_equals: 2\n",
        )
        .unwrap_err();
        assert!(err.contains("exactly one of"), "{err}");
    }

    #[test]
    fn trim_and_case_on_every_text_value() {
        let file =
            pipeline("  - trim:\n      fields: [\"*\"]\n  - lowercase:\n      fields: [email]\n");
        let out = run_one(&file, json!({"email": "  Ada@X.COM ", "age": 3})).unwrap();
        assert_eq!(out, Outcome::Keep(json!({"email": "ada@x.com", "age": 3})));
    }

    #[test]
    fn title_case_keeps_double_spaces() {
        assert_eq!(title_case("aDA  lOVELACE"), "Ada  Lovelace");
    }

    #[test]
    fn rename_skips_missing_fields() {
        let file = pipeline("  - rename:\n      first_name: firstName\n");
        let out = run_one(&file, json!({"first_name": "Ada"})).unwrap();
        assert_eq!(out, Outcome::Keep(json!({"firstName": "Ada"})));
        let out = run_one(&file, json!({"other": 1})).unwrap();
        assert_eq!(out, Outcome::Keep(json!({"other": 1})));
    }

    #[test]
    fn select_keeps_only_named_fields_including_nested() {
        let file = pipeline("  - select:\n      fields: [a, b.c]\n");
        let out = run_one(&file, json!({"a": 1, "b": {"c": 2, "d": 3}, "e": 4})).unwrap();
        assert_eq!(out, Outcome::Keep(json!({"a": 1, "b": {"c": 2}})));
    }

    #[test]
    fn to_number_reads_text_and_rejects_non_numbers() {
        let file = pipeline("  - to_number:\n      fields: [amount]\n");
        assert_eq!(
            run_one(&file, json!({"amount": "120.50"})).unwrap(),
            Outcome::Keep(json!({"amount": 120.5}))
        );
        let err = run_one(&file, json!({"amount": "abc"})).unwrap_err();
        assert_eq!(
            err,
            "step 1 (to_number): field 'amount': 'abc' is not a number"
        );
    }

    #[test]
    fn to_number_rejects_infinity_and_nan() {
        let file = pipeline("  - to_number:\n      fields: [x]\n");
        assert!(run_one(&file, json!({"x": "inf"})).is_err());
        assert!(run_one(&file, json!({"x": "NaN"})).is_err());
    }

    #[test]
    fn to_bool_accepts_common_words() {
        let file = pipeline("  - to_bool:\n      fields: [a, b]\n");
        let out = run_one(&file, json!({"a": "Yes", "b": "0"})).unwrap();
        assert_eq!(out, Outcome::Keep(json!({"a": true, "b": false})));
        assert!(run_one(&file, json!({"a": "maybe", "b": "0"})).is_err());
    }

    #[test]
    fn defaults_fill_missing_null_and_empty_only() {
        let file = pipeline("  - defaults:\n      country: NZ\n      tier: 1\n");
        let out = run_one(&file, json!({"country": null, "tier": ""})).unwrap();
        assert_eq!(out, Outcome::Keep(json!({"country": "NZ", "tier": 1})));
        let out = run_one(&file, json!({"country": "UK", "tier": 3})).unwrap();
        assert_eq!(out, Outcome::Keep(json!({"country": "UK", "tier": 3})));
    }

    #[test]
    fn conditional_compares_text_so_csv_numbers_match() {
        let file = pipeline(
            "  - conditional:\n      when: {field: amount, equals: 0}\n      set: {note: free}\n",
        );
        let out = run_one(&file, json!({"amount": "0"})).unwrap();
        assert_eq!(out, Outcome::Keep(json!({"amount": "0", "note": "free"})));
        let out = run_one(&file, json!({"amount": "5"})).unwrap();
        assert_eq!(out, Outcome::Keep(json!({"amount": "5"})));
    }

    #[test]
    fn combine_joins_and_removes_sources_by_default() {
        let file = pipeline("  - combine:\n      fields: [first, last]\n      into: full\n");
        let out = run_one(&file, json!({"first": "Ada", "last": "Lovelace"})).unwrap();
        assert_eq!(out, Outcome::Keep(json!({"full": "Ada Lovelace"})));
    }

    #[test]
    fn combine_can_keep_sources_and_use_a_separator() {
        let file = pipeline(
            "  - combine:\n      fields: [a, b]\n      into: ab\n      separator: \"-\"\n      keep_sources: true\n",
        );
        let out = run_one(&file, json!({"a": "x", "b": 2})).unwrap();
        assert_eq!(out, Outcome::Keep(json!({"a": "x", "b": 2, "ab": "x-2"})));
    }

    #[test]
    fn split_trims_parts_and_nulls_empty_ones() {
        let file = pipeline(
            "  - split:\n      field: name\n      into: [first, last]\n      separator: \",\"\n",
        );
        let out = run_one(&file, json!({"name": "Lovelace, Ada"})).unwrap();
        assert_eq!(
            out,
            Outcome::Keep(json!({"first": "Lovelace", "last": "Ada"}))
        );
        let out = run_one(&file, json!({"name": "Ada"})).unwrap();
        assert_eq!(out, Outcome::Keep(json!({"first": "Ada", "last": null})));
    }

    #[test]
    fn split_with_too_many_parts_is_rejected() {
        let file =
            pipeline("  - split:\n      field: name\n      into: [a]\n      separator: \",\"\n");
        let err = run_one(&file, json!({"name": "x,y"})).unwrap_err();
        assert!(err.contains("has 2 parts"), "{err}");
    }

    #[test]
    fn map_replaces_listed_values_only() {
        let file =
            pipeline("  - map:\n      field: status\n      values: {A: active, I: inactive}\n");
        assert_eq!(
            run_one(&file, json!({"status": "A"})).unwrap(),
            Outcome::Keep(json!({"status": "active"}))
        );
        assert_eq!(
            run_one(&file, json!({"status": "Z"})).unwrap(),
            Outcome::Keep(json!({"status": "Z"}))
        );
    }

    #[test]
    fn filter_drops_non_matching_records_without_rejecting() {
        let file = pipeline("  - filter:\n      field: status\n      not_equals: inactive\n");
        assert_eq!(
            run_one(&file, json!({"status": "inactive"})).unwrap(),
            Outcome::Drop
        );
        assert_eq!(
            run_one(&file, json!({"status": "active"})).unwrap(),
            Outcome::Keep(json!({"status": "active"}))
        );
    }

    #[test]
    fn calculate_adds_and_rounds() {
        let file = pipeline(
            "  - calculate:\n      into: total\n      operation: add\n      fields: [a, b]\n      decimals: 2\n",
        );
        let out = run_one(&file, json!({"a": 0.1, "b": 0.2})).unwrap();
        assert_eq!(
            out,
            Outcome::Keep(json!({"a": 0.1, "b": 0.2, "total": 0.3}))
        );
        let out = run_one(&file, json!({"a": 2, "b": 3})).unwrap();
        assert_eq!(out, Outcome::Keep(json!({"a": 2, "b": 3, "total": 5})));
    }

    #[test]
    fn calculate_needs_numbers_not_text() {
        let file = pipeline(
            "  - calculate:\n      into: t\n      operation: multiply\n      fields: [a, b]\n",
        );
        let err = run_one(&file, json!({"a": "2", "b": 3})).unwrap_err();
        assert!(err.contains("add a to_number step"), "{err}");
    }

    #[test]
    fn divide_by_zero_is_rejected() {
        let file = pipeline(
            "  - calculate:\n      into: r\n      operation: divide\n      fields: [a, b]\n",
        );
        let err = run_one(&file, json!({"a": 1, "b": 0})).unwrap_err();
        assert!(err.contains("division by zero"), "{err}");
    }

    #[test]
    fn subtract_needs_exactly_two_fields() {
        let err = parse_pipeline(
            "pipeline:\n  - calculate:\n      into: r\n      operation: subtract\n      fields: [a]\n",
        )
        .unwrap_err();
        assert!(err.contains("exactly two fields"), "{err}");
    }

    #[test]
    fn run_collects_rejections_and_keeps_going() {
        let file = pipeline(
            "  - to_number:\n      fields: [n]\n  - filter:\n      field: n\n      not_equals: 2\n",
        );
        let items = ["1", "x", "2"].iter().enumerate().map(|(i, text)| {
            Item::Row(tpt_data_core::Row {
                number: i as u64 + 1,
                record: json!({"n": text}),
            })
        });
        let result = run(&file, items);
        assert_eq!(result.read, 3);
        assert_eq!(result.kept, vec![(1, json!({"n": 1}))]);
        assert_eq!(result.filtered, 1);
        assert_eq!(result.rejected.len(), 1);
        assert_eq!(result.rejected[0].number, 2);
    }

    #[test]
    fn headers_keep_input_order_then_new_fields() {
        let input = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let kept = vec![(1, json!({"a": 1, "c": 3, "z": 9}))];
        assert_eq!(output_headers(Some(&input), &kept), vec!["a", "c", "z"]);
    }
}
