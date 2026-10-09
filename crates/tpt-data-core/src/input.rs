//! Reading CSV, JSON and JSON Lines input.
//!
//! CSV, JSON Lines and JSON arrays are all streamed, one record at a time. A
//! JSON array is split into records by its brackets and quotes, and each
//! record is parsed on its own, so the whole file is never held in memory.
//!
//! A record that is valid JSON but the wrong shape, or that does not parse,
//! is counted as invalid and the run goes on. A file that ends before the
//! array is closed, or has text after its closing `]`, cannot be recovered
//! from, so the run stops with an input error. Records before that point have
//! already been checked by then.
//!
//! A CSV header with dots becomes nested fields: `customer.email` is read as
//! `{"customer": {"email": ...}}`. This matches how JSON records are read, so
//! one schema path works for both.

use std::cell::RefCell;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read};
use std::path::Path;
use std::rc::Rc;

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
    /// Set when the input breaks off part-way through. Check it after the
    /// last item: [`Input::failure`].
    failure: Failure,
}

impl Input {
    /// The error that stopped the input early, if any. Only a JSON array can
    /// break off part-way; the other formats always read to the end.
    pub fn failure(&self) -> Option<DataError> {
        self.failure.borrow_mut().take()
    }
}

impl Iterator for Input {
    type Item = Item;

    fn next(&mut self) -> Option<Item> {
        self.items.next()
    }
}

/// Shared between a streaming reader and its [`Input`], so the caller can see
/// why the stream stopped.
type Failure = Rc<RefCell<Option<DataError>>>;

/// Open an input file in the given format.
///
/// Fails when the file cannot be opened, or when a JSON file does not start
/// as an array. Problems with individual records become [`Item::Malformed`].
/// A JSON array that breaks off part-way is reported by [`Input::failure`].
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
        failure: Failure::default(),
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
        failure: Failure::default(),
    }
}

fn open_json(mut reader: BufReader<File>, location: &str) -> Result<Input, DataError> {
    // The opening `[` is checked here, so a file that is not an array fails
    // before any output is written.
    skip_whitespace(&mut reader).map_err(|e| read_error(location, &e))?;
    match peek(&mut reader).map_err(|e| read_error(location, &e))? {
        Some(b'[') => reader.consume(1),
        Some(_) => {
            return Err(DataError::new(
                "JSON input is not an array",
                location,
                "a .json file must hold an array of records",
                "wrap the records in [ ], or use .jsonl for one record per line",
            ))
        }
        None => {
            return Err(DataError::new(
                "input is not valid JSON",
                location,
                "the file is empty",
                "a .json file must hold an array of records",
            ))
        }
    }

    let failure = Failure::default();
    let items = JsonArrayItems {
        reader,
        location: location.to_string(),
        number: 0,
        closed: false,
        ended: false,
        failure: Rc::clone(&failure),
    };
    Ok(Input {
        format: Format::Json,
        headers: None,
        items: Box::new(items),
        failure,
    })
}

fn read_error(location: &str, e: &io::Error) -> DataError {
    DataError::new(
        "cannot read input",
        location,
        &e.to_string(),
        "check the file is readable and not changing while it is read",
    )
}

/// Reads the records of a JSON array one at a time. The opening `[` has
/// already been read by [`open_json`].
///
/// Each record is found by its brackets and quotes, then parsed on its own.
/// A record that does not parse, or is not an object, is [`Item::Malformed`]
/// and the run goes on. A break in the array's own structure cannot be
/// recovered from, so the stream stops and the error goes to `failure`.
struct JsonArrayItems {
    reader: BufReader<File>,
    location: String,
    number: u64,
    /// The closing `]` has been read. Only whitespace may follow it.
    closed: bool,
    ended: bool,
    failure: Failure,
}

impl Iterator for JsonArrayItems {
    type Item = Item;

    fn next(&mut self) -> Option<Item> {
        if self.ended {
            return None;
        }
        match self.next_record() {
            Ok(Some(bytes)) => {
                self.number += 1;
                let number = self.number;
                Some(match serde_json::from_slice::<Value>(&bytes) {
                    Ok(value) if value.is_object() => Item::Row(Row {
                        number,
                        record: value,
                    }),
                    Ok(_) => Item::Malformed {
                        number,
                        message: "record is JSON, but not an object".to_string(),
                    },
                    Err(e) => Item::Malformed {
                        number,
                        message: format!("record is not valid JSON: {e}"),
                    },
                })
            }
            Ok(None) => {
                self.ended = true;
                None
            }
            Err(reason) => {
                self.ended = true;
                *self.failure.borrow_mut() = Some(DataError::new(
                    "input is not valid JSON",
                    &self.location,
                    &reason,
                    "check the JSON syntax after the last good record, or use --format jsonl for one record per line",
                ));
                None
            }
        }
    }
}

impl JsonArrayItems {
    /// The bytes of the next record, or `None` once the array is closed and
    /// nothing else follows it.
    fn next_record(&mut self) -> Result<Option<Vec<u8>>, String> {
        if self.closed {
            skip_whitespace(&mut self.reader).map_err(|e| e.to_string())?;
            return match peek(&mut self.reader).map_err(|e| e.to_string())? {
                None => Ok(None),
                Some(_) => Err("there is text after the closing ]".to_string()),
            };
        }
        if self.number == 0 {
            skip_whitespace(&mut self.reader).map_err(|e| e.to_string())?;
            if peek(&mut self.reader).map_err(|e| e.to_string())? == Some(b']') {
                // An empty array, `[]`.
                self.reader.consume(1);
                self.closed = true;
                return self.next_record();
            }
        }

        // Collect bytes up to the next `,` or `]` that is outside a string
        // and outside any nested brackets.
        let mut bytes = Vec::new();
        let mut depth = 0usize;
        let mut in_string = false;
        let mut escaped = false;
        while let Some(c) = peek(&mut self.reader).map_err(|e| e.to_string())? {
            if !in_string && depth == 0 && (c == b',' || c == b']') {
                break;
            }
            self.reader.consume(1);
            bytes.push(c);
            if in_string {
                if escaped {
                    escaped = false;
                } else if c == b'\\' {
                    escaped = true;
                } else if c == b'"' {
                    in_string = false;
                }
            } else {
                match c {
                    b'"' => in_string = true,
                    b'[' | b'{' => depth += 1,
                    b']' | b'}' => depth = depth.saturating_sub(1),
                    _ => {}
                }
            }
        }

        match peek(&mut self.reader).map_err(|e| e.to_string())? {
            None => Err("the file ends before the array is closed".to_string()),
            Some(b',') => {
                self.reader.consume(1);
                Ok(Some(bytes))
            }
            Some(_) => {
                // The `]` that closes the array. Checked on the next call.
                self.reader.consume(1);
                self.closed = true;
                Ok(Some(bytes))
            }
        }
    }
}

/// Skip spaces, tabs and line breaks.
fn skip_whitespace(reader: &mut impl BufRead) -> io::Result<()> {
    while let Some(c) = peek(reader)? {
        if !matches!(c, b' ' | b'\t' | b'\r' | b'\n') {
            break;
        }
        reader.consume(1);
    }
    Ok(())
}

/// The next byte, without taking it. `None` at the end of the file.
fn peek(reader: &mut impl BufRead) -> io::Result<Option<u8>> {
    Ok(reader.fill_buf()?.first().copied())
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Write `text` to a temp file, read it as JSON, and collect what comes out.
    fn read_json(name: &str, text: &str) -> Result<(Vec<Item>, Option<DataError>), DataError> {
        let path =
            std::env::temp_dir().join(format!("tpt-input-{}-{name}.json", std::process::id()));
        std::fs::write(&path, text).unwrap();
        let input = open(&path, Format::Json);
        let result = input.map(|mut input| {
            let items: Vec<Item> = input.by_ref().collect();
            (items, input.failure())
        });
        std::fs::remove_file(&path).ok();
        result
    }

    #[test]
    fn records_with_brackets_commas_and_escaped_quotes_are_split_correctly() {
        let text = r#"[ {"a": "x, ] } \" [", "b": [1, {"c": 2}]} ,
            {"a": "plain"} ]"#;
        let (items, failure) = read_json("brackets", text).unwrap();
        assert!(failure.is_none());
        assert_eq!(items.len(), 2);
        let Item::Row(first) = &items[0] else {
            panic!("expected a row")
        };
        assert_eq!(first.number, 1);
        assert_eq!(first.record["a"], "x, ] } \" [");
        assert_eq!(first.record["b"][1]["c"], 2);
        let Item::Row(second) = &items[1] else {
            panic!("expected a row")
        };
        assert_eq!(second.number, 2);
        assert_eq!(second.record["a"], "plain");
    }

    #[test]
    fn bad_record_is_malformed_and_the_run_goes_on() {
        let (items, failure) = read_json("bad-record", r#"[{"a":1}, {"a":}, 5, {"a":2}]"#).unwrap();
        assert!(failure.is_none());
        assert_eq!(items.len(), 4);
        assert!(matches!(items[0], Item::Row(_)));
        assert!(
            matches!(&items[1], Item::Malformed { number: 2, message } if message.contains("not valid JSON"))
        );
        assert!(
            matches!(&items[2], Item::Malformed { number: 3, message } if message.contains("not an object"))
        );
        assert!(matches!(items[3], Item::Row(_)));
    }

    #[test]
    fn truncated_array_yields_good_records_then_a_failure() {
        let (items, failure) = read_json("truncated", "[{\"a\":1},{\"a\":2}").unwrap();
        assert_eq!(
            items.len(),
            1,
            "the second record is never closed, so it is not emitted"
        );
        let failure = failure.expect("a failure is recorded");
        assert_eq!(failure.what, "input is not valid JSON");
        assert!(
            failure.why.contains("ends before the array is closed"),
            "{}",
            failure.why
        );
    }

    #[test]
    fn missing_comma_makes_one_malformed_record() {
        // The two objects have no comma between them, so they are read as one
        // record, which does not parse. The array itself is still closed.
        let (items, failure) = read_json("comma", r#"[{"a":1} {"a":2}]"#).unwrap();
        assert_eq!(items.len(), 1);
        assert!(matches!(items[0], Item::Malformed { number: 1, .. }));
        assert!(failure.is_none());
    }

    #[test]
    fn text_after_the_closing_bracket_is_a_failure_and_keeps_the_last_record() {
        let (items, failure) = read_json("trailing", r#"[{"a":1}] oops"#).unwrap();
        assert_eq!(items.len(), 1, "the last record is not lost");
        let failure = failure.expect("trailing text is recorded");
        assert!(failure.why.contains("after the closing"), "{}", failure.why);
    }

    #[test]
    fn empty_array_has_no_records_and_no_failure() {
        let (items, failure) = read_json("empty", " [ \n ] \n").unwrap();
        assert!(items.is_empty());
        assert!(failure.is_none());
    }

    #[test]
    fn object_at_top_level_is_rejected_when_opened() {
        let err = read_json("object", r#"{"a":1}"#).unwrap_err();
        assert_eq!(err.what, "JSON input is not an array");
    }
}
