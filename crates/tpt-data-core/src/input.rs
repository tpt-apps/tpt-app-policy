//! Reading CSV, JSON and JSON Lines input.
//!
//! CSV and JSON Lines are streamed, one record at a time. A JSON array is
//! read in full, because a JSON array is not streamable without a dedicated
//! parser. Use JSON Lines for files too big to hold in memory.
//!
//! A CSV header with dots becomes nested fields: `customer.email` is read as
//! `{"customer": {"email": ...}}`. This matches how JSON records are read, so
//! one schema path works for both.

use std::fs::File;
use std::io::{self, BufRead, BufReader, Read};
use std::path::Path;

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::DataError;

/// A supported input format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Csv,
    Json,
    Jsonl,
}

impl Format {
    /// Parse a format name as given on the command line.
    pub fn parse(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "csv" => Some(Self::Csv),
            "json" => Some(Self::Json),
            "jsonl" | "ndjson" => Some(Self::Jsonl),
            _ => None,
        }
    }

    /// Guess the format from the file extension.
    pub fn from_path(path: &Path) -> Option<Self> {
        Self::parse(path.extension()?.to_str()?)
    }

    /// The name used in reports and on the command line.
    pub fn name(self) -> &'static str {
        match self {
            Self::Csv => "csv",
            Self::Json => "json",
            Self::Jsonl => "jsonl",
        }
    }
}

/// One record, with its number. Numbers start at 1 and do not count the CSV
/// header, so `Row 182` means the 182nd record.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub number: u64,
    /// A JSON object. CSV cells are text, with empty cells as null.
    pub record: Value,
}

/// One item from the input: a record, or a record that could not be read.
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Row(Row),
    /// The record could not be read at all (bad CSV row, bad JSON line).
    /// It is counted as invalid, and the run goes on.
    Malformed {
        number: u64,
        message: String,
    },
}

impl Item {
    pub fn number(&self) -> u64 {
        match self {
            Self::Row(row) => row.number,
            Self::Malformed { number, .. } => *number,
        }
    }
}

/// An open input. Iterate it to read the records.
pub struct Input {
    pub format: Format,
    /// Column names, for CSV. `None` for JSON formats.
    pub headers: Option<Vec<String>>,
    items: Box<dyn Iterator<Item = Item>>,
}

impl Iterator for Input {
    type Item = Item;

    fn next(&mut self) -> Option<Item> {
        self.items.next()
    }
}

/// Open an input file in the given format.
///
/// Fails when the file cannot be opened, or when a JSON array is not valid
/// JSON. Other problems with individual records become [`Item::Malformed`].
pub fn open(path: &Path, format: Format) -> Result<Input, DataError> {
    let location = path.display().to_string();
    let file = File::open(path).map_err(|e| {
        DataError::new(
            "cannot read input",
            &location,
            &e.to_string(),
            "check the path is right and the file is readable",
        )
    })?;
    let reader = BufReader::new(file);
    match format {
        Format::Csv => open_csv(reader, &location),
        Format::Jsonl => Ok(open_jsonl(reader)),
        Format::Json => open_json(reader, &location),
    }
}

fn open_csv(reader: impl Read + 'static, location: &str) -> Result<Input, DataError> {
    let mut csv = csv::ReaderBuilder::new()
        .has_headers(true)
        .trim(csv::Trim::Headers)
        .from_reader(reader);
    let headers: Vec<String> = csv
        .headers()
        .map_err(|e| {
            DataError::new(
                "cannot read CSV header",
                location,
                &e.to_string(),
                "make sure the first line is the column names",
            )
        })?
        .iter()
        .map(str::to_string)
        .collect();

    let header_copy = headers.clone();
    let mut number = 0u64;
    let items = csv.into_records().map(move |result| {
        number += 1;
        match result {
            Ok(cells) => {
                let cells: Vec<String> = cells.iter().map(str::to_string).collect();
                let record = Value::Object(record_from_columns(&header_copy, &cells));
                Item::Row(Row { number, record })
            }
            Err(e) => Item::Malformed {
                number,
                message: format!("CSV row could not be read: {e}"),
            },
        }
    });
    Ok(Input {
        format: Format::Csv,
        headers: Some(headers),
        items: Box::new(items),
    })
}

fn open_jsonl(reader: impl BufRead + 'static) -> Input {
    let mut number = 0u64;
    let items = reader.lines().filter_map(move |line| {
        let line = match line {
            Ok(line) => line,
            Err(e) => {
                number += 1;
                return Some(Item::Malformed {
                    number,
                    message: format!("line could not be read: {e}"),
                });
            }
        };
        if line.trim().is_empty() {
            return None;
        }
        number += 1;
        Some(match serde_json::from_str::<Value>(&line) {
            Ok(value) if value.is_object() => Item::Row(Row {
                number,
                record: value,
            }),
            Ok(_) => Item::Malformed {
                number,
                message: "line is JSON, but not an object".to_string(),
            },
            Err(e) => Item::Malformed {
                number,
                message: format!("line is not valid JSON: {e}"),
            },
        })
    });
    Input {
        format: Format::Jsonl,
        headers: None,
        items: Box::new(items),
    }
}

fn open_json(reader: impl Read, location: &str) -> Result<Input, DataError> {
    let value: Value = serde_json::from_reader(reader).map_err(|e| {
        DataError::new(
            "input is not valid JSON",
            location,
            &e.to_string(),
            "check the JSON syntax at the line shown, or use --format jsonl for one record per line",
        )
    })?;
    let Value::Array(records) = value else {
        return Err(DataError::new(
            "JSON input is not an array",
            location,
            "a .json file must hold an array of records",
            "wrap the records in [ ], or use .jsonl for one record per line",
        ));
    };
    let items = records.into_iter().enumerate().map(|(i, value)| {
        let number = i as u64 + 1;
        if value.is_object() {
            Item::Row(Row {
                number,
                record: value,
            })
        } else {
            Item::Malformed {
                number,
                message: "record is JSON, but not an object".to_string(),
            }
        }
    });
    Ok(Input {
        format: Format::Json,
        headers: None,
        items: Box::new(items),
    })
}

/// Build a record from CSV columns. Dotted headers become nested objects.
/// Empty cells become null, so a required field left blank is reported as missing.
pub fn record_from_columns(headers: &[String], cells: &[String]) -> Map<String, Value> {
    let mut root = Map::new();
    for (header, cell) in headers.iter().zip(cells) {
        let value = if cell.is_empty() {
            Value::Null
        } else {
            Value::String(cell.clone())
        };
        set_path(&mut root, header, value);
    }
    root
}

pub(crate) fn set_path(root: &mut Map<String, Value>, path: &str, value: Value) {
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

/// SHA-256 of a file, as lowercase hex. Read in chunks, so large files are fine.
pub fn sha256_file(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}
