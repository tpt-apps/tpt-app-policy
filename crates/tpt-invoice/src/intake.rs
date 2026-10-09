//! Reading invoice files. JSON and XML hold one invoice each. CSV holds many.
//!
//! A CSV file is read like this:
//!
//! - Each row is one line of one invoice. Rows that share a `supplier.tax_id`
//!   and `invoice_number` belong to the same invoice, wherever they are in the
//!   file. An invoice is placed where its first row is.
//! - Columns named `line.<field>` (for example `line.quantity`) hold the line
//!   item for that row. Any other column is an invoice field. Dotted names
//!   nest, so `supplier.tax_id` is `{"supplier": {"tax_id": ...}}`.
//! - The invoice fields must be the same on every row of an invoice. A row
//!   that disagrees fails the whole invoice, and the message names the field.
//! - A row that cannot be read fails that row alone. It is not grouped.
//! - A row with no `invoice_number` or `supplier.tax_id` cannot be grouped, so
//!   it is an invoice on its own. The schema then reports the missing fields.
//!
//! Every CSV cell is text, so the schema is checked in text mode, the same as XML.

use std::collections::HashMap;
use std::path::Path;

use serde_json::{Map, Value};
use tpt_document::{read_document, DocError, DocFormat, Parsed};

use crate::duplicate_key;

/// One invoice, ready for checking.
#[derive(Debug)]
pub struct Invoice {
    /// How the invoice is named in the console report, e.g. `batch.csv row 2`.
    pub label: String,
    /// The file name stem used for its result file, e.g. `batch.row-2`.
    pub out_stem: String,
    pub parsed: Parsed,
}

/// Read the invoices in one file. A file that cannot be read at all is an error.
pub fn read_invoices(path: &Path) -> Result<Vec<Invoice>, DocError> {
    let is_csv = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("csv"));
    if is_csv {
        return read_csv(path);
    }
    let parsed = read_document(path)?;
    Ok(vec![Invoice {
        label: path.display().to_string(),
        out_stem: file_stem(path),
        parsed,
    }])
}

fn file_stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "invoice".to_string())
}

/// One invoice being built from its rows.
struct Group {
    /// The first row's number, counting data rows from 1.
    first_row: u64,
    /// The invoice fields, from the first row.
    fields: Option<Map<String, Value>>,
    lines: Vec<Value>,
    /// Set when the rows cannot form one invoice.
    error: Option<String>,
}

fn read_csv(path: &Path) -> Result<Vec<Invoice>, DocError> {
    let location = path.display().to_string();
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .trim(csv::Trim::All)
        .from_path(path)
        .map_err(|e| DocError {
            what: "cannot read document".to_string(),
            location: location.clone(),
            why: e.to_string(),
            fix: "check the path is right and the file is readable".to_string(),
        })?;
    let headers: Vec<String> = reader
        .headers()
        .map_err(|e| DocError {
            what: "cannot read CSV header".to_string(),
            location: location.clone(),
            why: e.to_string(),
            fix: "make sure the first line is the column names".to_string(),
        })?
        .iter()
        .map(str::to_string)
        .collect();

    let mut groups: Vec<Group> = Vec::new();
    let mut by_key: HashMap<String, usize> = HashMap::new();
    for (index, result) in reader.records().enumerate() {
        let row = index as u64 + 1;
        let cells = match result {
            Ok(cells) => cells,
            Err(e) => {
                groups.push(Group {
                    first_row: row,
                    fields: None,
                    lines: Vec::new(),
                    error: Some(format!("row could not be read: {e}")),
                });
                continue;
            }
        };
        let mut record = record_from_cells(&headers, cells.iter());
        let line = record.remove("line");
        let key =
            duplicate_key(&Value::Object(record.clone())).unwrap_or_else(|| format!("row {row}"));
        let index = match by_key.get(&key) {
            Some(&i) => i,
            None => {
                by_key.insert(key.clone(), groups.len());
                groups.push(Group {
                    first_row: row,
                    fields: Some(record.clone()),
                    lines: Vec::new(),
                    error: None,
                });
                groups.len() - 1
            }
        };
        let group = &mut groups[index];
        if group.error.is_none() {
            if let Some(first) = &group.fields {
                if let Some(field) = differing_field(first, &record) {
                    group.error = Some(format!(
                        "rows {} and {row} of this invoice disagree on '{field}'",
                        group.first_row
                    ));
                }
            }
        }
        if let Some(line) = line {
            group.lines.push(line);
        }
    }

    Ok(groups
        .into_iter()
        .map(|group| {
            let stem = format!("{}.row-{}", file_stem(path), group.first_row);
            let label = format!("{location} row {}", group.first_row);
            let parsed = match (group.error, group.fields) {
                (Some(message), _) => Parsed::Malformed {
                    format: DocFormat::Csv,
                    message,
                },
                // Every grouped row sets the fields, so this only guards the match.
                (None, None) => Parsed::Malformed {
                    format: DocFormat::Csv,
                    message: "the row has no invoice fields".to_string(),
                },
                (None, Some(mut fields)) => {
                    fields.insert("lines".to_string(), Value::Array(group.lines));
                    Parsed::Ok {
                        format: DocFormat::Csv,
                        value: Value::Object(fields),
                    }
                }
            };
            Invoice {
                label,
                out_stem: stem,
                parsed,
            }
        })
        .collect())
}

/// Build a record from one row. Empty cells are null, so a blank required
/// field is reported as missing.
fn record_from_cells<'a>(
    headers: &[String],
    cells: impl Iterator<Item = &'a str>,
) -> Map<String, Value> {
    let mut root = Map::new();
    for (header, cell) in headers.iter().zip(cells) {
        let value = if cell.is_empty() {
            Value::Null
        } else {
            Value::String(cell.to_string())
        };
        set_path(&mut root, header, value);
    }
    root
}

/// Put `value` at a dotted path, creating objects on the way.
fn set_path(root: &mut Map<String, Value>, path: &str, value: Value) {
    let mut parts = path.split('.').peekable();
    let mut current = root;
    while let Some(part) = parts.next() {
        if parts.peek().is_none() {
            current.insert(part.to_string(), value);
            return;
        }
        let entry = current
            .entry(part.to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        if !entry.is_object() {
            *entry = Value::Object(Map::new());
        }
        current = entry.as_object_mut().expect("just made an object");
    }
}

/// The first top-level field whose value differs between two rows of the same
/// invoice. The line item has already been taken out of both records.
fn differing_field(first: &Map<String, Value>, other: &Map<String, Value>) -> Option<String> {
    let mut names: Vec<&String> = first.keys().chain(other.keys()).collect();
    names.sort();
    names.dedup();
    names
        .into_iter()
        .find(|name| first.get(*name) != other.get(*name))
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cells(headers: &[&str], row: &[&str]) -> Map<String, Value> {
        let headers: Vec<String> = headers.iter().map(|h| h.to_string()).collect();
        record_from_cells(&headers, row.iter().copied())
    }

    #[test]
    fn dotted_headers_nest_and_empty_cells_are_null() {
        let record = cells(
            &["supplier.tax_id", "currency", "note"],
            &["123", "NZD", ""],
        );
        assert_eq!(record["supplier"]["tax_id"], "123");
        assert_eq!(record["currency"], "NZD");
        assert!(record["note"].is_null());
    }

    #[test]
    fn differing_field_finds_the_first_difference() {
        let a = cells(&["invoice_number", "currency"], &["1", "NZD"]);
        let b = cells(&["invoice_number", "currency"], &["1", "NZD"]);
        assert_eq!(differing_field(&a, &b), None);
        let c = cells(&["invoice_number", "currency"], &["1", "NZD"]);
        let d = cells(&["invoice_number", "currency"], &["1", "AUD"]);
        assert_eq!(differing_field(&c, &d), Some("currency".to_string()));
    }
}
