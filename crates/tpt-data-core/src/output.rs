//! Writing valid and invalid records.
//!
//! Valid records are written in the input's format, so a CSV goes in as CSV
//! and comes out as CSV with the same columns. Invalid records always go to a
//! JSON Lines file, one object per record, with its row number and errors. A
//! JSON Lines file is easy to read back and to fix.

use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::Path;

use serde_json::{json, Value};

use crate::input::Format;
use crate::validate::Outcome;

/// Writes valid records in the input format.
pub struct ValidWriter {
    headers: Vec<String>,
    count: u64,
    sink: Sink,
}

enum Sink {
    Csv(Box<csv::Writer<BufWriter<File>>>),
    Json(BufWriter<File>),
    Jsonl(BufWriter<File>),
}

impl ValidWriter {
    /// Create the file. For CSV, `headers` are the input's columns, in order.
    pub fn create(path: &Path, format: Format, headers: &[String]) -> io::Result<Self> {
        let mut file = BufWriter::new(File::create(path)?);
        let sink = match format {
            Format::Csv => {
                let mut writer = csv::Writer::from_writer(file);
                writer.write_record(headers)?;
                Sink::Csv(Box::new(writer))
            }
            Format::Json => {
                file.write_all(b"[")?;
                Sink::Json(file)
            }
            Format::Jsonl => Sink::Jsonl(file),
        };
        Ok(Self {
            headers: headers.to_vec(),
            count: 0,
            sink,
        })
    }

    /// Write one valid record, as read from the input.
    pub fn write(&mut self, record: &Value) -> io::Result<()> {
        match &mut self.sink {
            Sink::Csv(writer) => {
                let cells: Vec<String> = self
                    .headers
                    .iter()
                    .map(|header| cell_text(record, header))
                    .collect();
                writer.write_record(cells)?;
            }
            Sink::Json(file) => {
                let separator: &[u8] = if self.count > 0 { b",\n" } else { b"\n" };
                file.write_all(separator)?;
                file.write_all(serde_json::to_string(record)?.as_bytes())?;
            }
            Sink::Jsonl(file) => {
                file.write_all(serde_json::to_string(record)?.as_bytes())?;
                file.write_all(b"\n")?;
            }
        }
        self.count += 1;
        Ok(())
    }

    /// Finish the file. Must be called, or a JSON array is left open.
    pub fn finish(self) -> io::Result<()> {
        match self.sink {
            Sink::Csv(mut writer) => writer.flush(),
            Sink::Json(mut file) => {
                if self.count > 0 {
                    file.write_all(b"\n")?;
                }
                file.write_all(b"]\n")?;
                file.flush()
            }
            Sink::Jsonl(mut file) => file.flush(),
        }
    }
}

/// The text of one CSV cell. Nulls and missing fields become empty text.
fn cell_text(record: &Value, path: &str) -> String {
    match tpt_schema::lookup(record, path) {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(text)) => text.clone(),
        Some(other) => other.to_string(),
    }
}

/// Writes invalid records, one JSON object per line.
pub struct InvalidWriter {
    file: BufWriter<File>,
}

impl InvalidWriter {
    pub fn create(path: &Path) -> io::Result<Self> {
        Ok(Self {
            file: BufWriter::new(File::create(path)?),
        })
    }

    /// Write one invalid record with its row number and errors.
    pub fn write(&mut self, outcome: &Outcome) -> io::Result<()> {
        let errors: Vec<Value> = outcome
            .errors
            .iter()
            .map(|e| json!({ "field": e.field, "reason": e.reason }))
            .collect();
        let line = json!({
            "row": outcome.number,
            "errors": errors,
            "record": outcome.record.clone().unwrap_or(Value::Null),
        });
        self.file
            .write_all(serde_json::to_string(&line)?.as_bytes())?;
        self.file.write_all(b"\n")
    }

    pub fn finish(mut self) -> io::Result<()> {
        self.file.flush()
    }
}
