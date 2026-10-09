//! Schema definitions and record checks (spec §34, Phase 3).
//!
//! A schema lists fields in order. Each field has a type and optional rules.
//! A record is a JSON object. A field name is a dotted path into the record,
//! and numeric segments index into lists, as in the policy engine.
//!
//! Two modes check records:
//!
//! - [`Mode::Strict`] for JSON input: values must already have the right JSON
//!   type. `"5"` is not an integer.
//! - [`Mode::Text`] for CSV input: every value arrives as text, so `"5"` is
//!   read as an integer. An empty cell counts as missing.

use std::collections::HashMap;
use std::fmt;

use regex::Regex;
use serde::Deserialize;
use serde_json::{Number, Value};

/// Version of this crate, recorded in reports so decisions can be reproduced.
pub const SCHEMA_ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");

/// A schema error. Says what failed, where, why, and how to fix it (§29.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaError {
    pub what: String,
    pub location: String,
    pub why: String,
    pub fix: String,
}

impl SchemaError {
    fn new(what: &str, location: &str, why: &str, fix: &str) -> Self {
        Self {
            what: what.to_string(),
            location: location.to_string(),
            why: why.to_string(),
            fix: fix.to_string(),
        }
    }
}

impl fmt::Display for SchemaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "error: {} at {}\n  why: {}\n  fix: {}",
            self.what, self.location, self.why, self.fix
        )
    }
}

impl std::error::Error for SchemaError {}

/// The type a field must have.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldType {
    String,
    Integer,
    Number,
    Boolean,
}

impl FieldType {
    fn parse(name: &str) -> Option<Self> {
        match name {
            "string" => Some(Self::String),
            "integer" => Some(Self::Integer),
            "number" => Some(Self::Number),
            "boolean" => Some(Self::Boolean),
            _ => None,
        }
    }

    /// The name used in the schema file and in messages.
    pub fn name(self) -> &'static str {
        match self {
            Self::String => "string",
            Self::Integer => "integer",
            Self::Number => "number",
            Self::Boolean => "boolean",
        }
    }
}

/// One field and its rules.
#[derive(Debug, Clone)]
pub struct Field {
    /// Dotted path into the record, e.g. `customer.email`.
    pub path: String,
    pub kind: FieldType,
    pub required: bool,
    /// Values must be unique across all records in one run.
    pub unique: bool,
    /// Allowed values. `None` means any value of the right type.
    pub allowed: Option<Vec<Value>>,
    /// Regular expression the text must match. String fields only.
    pub pattern: Option<Regex>,
    /// Smallest allowed value. Integer and number fields only.
    pub min: Option<f64>,
    /// Largest allowed value. Integer and number fields only.
    pub max: Option<f64>,
}

/// A parsed schema.
#[derive(Debug, Clone)]
pub struct Schema {
    pub name: String,
    pub version: String,
    pub fields: Vec<Field>,
}

/// How values are read. See the crate documentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Strict,
    Text,
}

/// A failed check on one field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldError {
    pub field: String,
    pub reason: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSchema {
    schema: String,
    #[serde(default)]
    version: Option<String>,
    fields: serde_yaml::Mapping,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawField {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    required: bool,
    #[serde(default)]
    unique: bool,
    #[serde(default, rename = "enum")]
    allowed: Option<Vec<serde_yaml::Value>>,
    #[serde(default)]
    pattern: Option<String>,
    #[serde(default)]
    min: Option<f64>,
    #[serde(default)]
    max: Option<f64>,
}

/// Parse a schema from YAML. Every problem found is reported with its path.
pub fn parse_schema(yaml: &str) -> Result<Schema, SchemaError> {
    let raw: RawSchema = serde_yaml::from_str(yaml).map_err(|e| {
        let location = e
            .location()
            .map(|l| format!("line {}, column {}", l.line(), l.column()))
            .unwrap_or_else(|| "the start of the file".to_string());
        SchemaError::new(
            "invalid schema YAML",
            &location,
            &e.to_string(),
            "check the indentation and the quotes near this line",
        )
    })?;

    if raw.schema.trim().is_empty() {
        return Err(SchemaError::new(
            "missing schema name",
            "schema",
            "every schema needs a name, which reports record",
            "set schema: to a short name, e.g. customer",
        ));
    }
    if raw.fields.is_empty() {
        return Err(SchemaError::new(
            "no fields",
            "fields",
            "a schema must check at least one field",
            "add at least one field under fields:",
        ));
    }

    let mut fields = Vec::new();
    for (key, value) in &raw.fields {
        let path = key.as_str().ok_or_else(|| {
            SchemaError::new(
                "field name is not text",
                "fields",
                "field names must be text, such as email or customer.email",
                "quote the field name",
            )
        })?;
        let location = format!("fields.{path}");
        if path.is_empty() || path.split('.').any(str::is_empty) {
            return Err(SchemaError::new(
                "invalid field name",
                &location,
                "field names are dotted paths with no empty parts",
                "use names such as email or customer.email",
            ));
        }
        let raw_field: RawField = serde_yaml::from_value(value.clone()).map_err(|e| {
            SchemaError::new(
                "invalid field definition",
                &location,
                &e.to_string(),
                "use keys: type, required, unique, enum, pattern, min, max",
            )
        })?;
        fields.push(build_field(path, &location, raw_field)?);
    }

    Ok(Schema {
        name: raw.schema,
        version: raw.version.unwrap_or_else(|| "unversioned".to_string()),
        fields,
    })
}

fn build_field(path: &str, location: &str, raw: RawField) -> Result<Field, SchemaError> {
    let kind = FieldType::parse(&raw.kind).ok_or_else(|| {
        SchemaError::new(
            "unknown type",
            &format!("{location}.type"),
            &format!("'{}' is not a type", raw.kind),
            "use one of: string, integer, number, boolean",
        )
    })?;

    let pattern = match raw.pattern {
        None => None,
        Some(text) => {
            if kind != FieldType::String {
                return Err(SchemaError::new(
                    "pattern on a non-text field",
                    &format!("{location}.pattern"),
                    "patterns match text, so they only apply to string fields",
                    "remove pattern, or change the type to string",
                ));
            }
            Some(Regex::new(&text).map_err(|e| {
                SchemaError::new(
                    "invalid pattern",
                    &format!("{location}.pattern"),
                    &e.to_string(),
                    "write a regular expression, e.g. '^[A-Z]{3}$'",
                )
            })?)
        }
    };

    if (raw.min.is_some() || raw.max.is_some())
        && !matches!(kind, FieldType::Integer | FieldType::Number)
    {
        return Err(SchemaError::new(
            "min or max on a non-numeric field",
            location,
            "min and max compare numbers",
            "remove min and max, or change the type to integer or number",
        ));
    }
    if let (Some(min), Some(max)) = (raw.min, raw.max) {
        if min > max {
            return Err(SchemaError::new(
                "min is greater than max",
                location,
                &format!("min {min} is greater than max {max}, so no value can pass"),
                "swap min and max, or correct one of them",
            ));
        }
    }

    let allowed = match raw.allowed {
        None => None,
        Some(list) => {
            if list.is_empty() {
                return Err(SchemaError::new(
                    "empty enum",
                    &format!("{location}.enum"),
                    "an empty list allows no value at all",
                    "list at least one allowed value, or remove enum",
                ));
            }
            let mut values = Vec::new();
            for item in list {
                let json = serde_json::to_value(&item).map_err(|e| {
                    SchemaError::new(
                        "invalid enum value",
                        &format!("{location}.enum"),
                        &e.to_string(),
                        "use plain values such as text, numbers or booleans",
                    )
                })?;
                if json.is_object() || json.is_array() {
                    return Err(SchemaError::new(
                        "invalid enum value",
                        &format!("{location}.enum"),
                        "enum values must be plain values, not lists or objects",
                        "list plain values such as text, numbers or booleans",
                    ));
                }
                values.push(json);
            }
            Some(values)
        }
    };

    Ok(Field {
        path: path.to_string(),
        kind,
        required: raw.required,
        unique: raw.unique,
        allowed,
        pattern,
        min: raw.min,
        max: raw.max,
    })
}

/// Look up a dotted path in a record. Numeric segments index into lists.
pub fn lookup<'a>(record: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = record;
    for segment in path.split('.') {
        current = match current {
            Value::Object(map) => map.get(segment)?,
            Value::Array(items) => items.get(segment.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(current)
}

/// Check one record against every field rule, except uniqueness.
/// Returns every failure, in schema order. An empty list means the record passes.
pub fn check_record(schema: &Schema, record: &Value, mode: Mode) -> Vec<FieldError> {
    let mut errors = Vec::new();
    for field in &schema.fields {
        let value = lookup(record, &field.path).filter(|v| !v.is_null());
        let Some(value) = value else {
            if field.required {
                errors.push(error(field, "is required"));
            }
            continue;
        };
        let typed = match coerce(field.kind, value, mode) {
            Ok(v) => v,
            Err(reason) => {
                errors.push(error(field, &reason));
                continue;
            }
        };
        if let Some(allowed) = &field.allowed {
            if !allowed.iter().any(|a| same_value(a, &typed)) {
                let list: Vec<String> = allowed.iter().map(show).collect();
                errors.push(error(
                    field,
                    &format!("{} is not one of [{}]", show(&typed), list.join(", ")),
                ));
            }
        }
        if let (Some(pattern), Value::String(text)) = (&field.pattern, &typed) {
            if !pattern.is_match(text) {
                errors.push(error(
                    field,
                    &format!(
                        "{} does not match the pattern {}",
                        show(&typed),
                        pattern.as_str()
                    ),
                ));
            }
        }
        if let Some(number) = typed.as_f64() {
            if let Some(min) = field.min {
                if number < min {
                    errors.push(error(
                        field,
                        &format!("{number} is below the minimum {min}"),
                    ));
                }
            }
            if let Some(max) = field.max {
                if number > max {
                    errors.push(error(
                        field,
                        &format!("{number} is above the maximum {max}"),
                    ));
                }
            }
        }
    }
    errors
}

fn error(field: &Field, reason: &str) -> FieldError {
    FieldError {
        field: field.path.clone(),
        reason: reason.to_string(),
    }
}

/// Convert a value to a field's type. Returns the converted value, or the reason it does not fit.
pub fn coerce(kind: FieldType, value: &Value, mode: Mode) -> Result<Value, String> {
    let fail = |what: &str| Err(format!("expected {what}, got {}", show(value)));
    match (kind, mode, value) {
        (FieldType::String, _, Value::String(_)) => Ok(value.clone()),
        (FieldType::String, _, _) => fail("text"),

        (FieldType::Integer, Mode::Strict, Value::Number(n)) if n.is_i64() || n.is_u64() => {
            Ok(value.clone())
        }
        (FieldType::Integer, Mode::Text, Value::String(s)) => match s.trim().parse::<i64>() {
            Ok(n) => Ok(Value::from(n)),
            Err(_) => fail("an integer"),
        },
        (FieldType::Integer, _, _) => fail("an integer"),

        (FieldType::Number, Mode::Strict, Value::Number(_)) => Ok(value.clone()),
        (FieldType::Number, Mode::Text, Value::String(s)) => match s.trim().parse::<f64>() {
            Ok(n) if n.is_finite() => Ok(Value::Number(Number::from_f64(n).expect("finite"))),
            _ => fail("a number"),
        },
        (FieldType::Number, _, _) => fail("a number"),

        (FieldType::Boolean, Mode::Strict, Value::Bool(_)) => Ok(value.clone()),
        (FieldType::Boolean, Mode::Text, Value::String(s)) => {
            match s.trim().to_ascii_lowercase().as_str() {
                "true" => Ok(Value::Bool(true)),
                "false" => Ok(Value::Bool(false)),
                _ => fail("true or false"),
            }
        }
        (FieldType::Boolean, _, _) => fail("true or false"),
    }
}

/// Equal as values, treating integers and floats with the same value as equal.
fn same_value(a: &Value, b: &Value) -> bool {
    match (a.as_f64(), b.as_f64()) {
        (Some(x), Some(y)) if a.is_number() && b.is_number() => x == y,
        _ => a == b,
    }
}

/// Short text form of a value for messages. Text is quoted.
fn show(value: &Value) -> String {
    match value {
        Value::String(s) => format!("'{s}'"),
        other => other.to_string(),
    }
}

/// Tracks values seen so far for `unique` fields, across one run.
#[derive(Debug, Default)]
pub struct UniqueIndex {
    seen: HashMap<(String, String), u64>,
}

impl UniqueIndex {
    pub fn new() -> Self {
        Self::default()
    }

    /// Check the unique fields of one record. `row` is the record's number, used
    /// in the message for a duplicate. Call it once per record that passes the
    /// other checks, so a rejected record does not block a later valid one.
    pub fn check(&mut self, schema: &Schema, record: &Value, row: u64) -> Vec<FieldError> {
        let mut errors = Vec::new();
        for field in schema.fields.iter().filter(|f| f.unique) {
            let Some(value) = lookup(record, &field.path).filter(|v| !v.is_null()) else {
                continue;
            };
            let key = (field.path.clone(), value_key(value));
            match self.seen.get(&key) {
                Some(first) => errors.push(error(
                    field,
                    &format!(
                        "duplicate value {} (first seen at row {first})",
                        show(value)
                    ),
                )),
                None => {
                    self.seen.insert(key, row);
                }
            }
        }
        errors
    }
}

fn value_key(value: &Value) -> String {
    match value {
        Value::String(s) => s.trim().to_string(),
        other => other.to_string(),
    }
}
