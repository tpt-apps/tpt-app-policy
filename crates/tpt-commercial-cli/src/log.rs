//! Local logs (spec §38). Logs go to stderr, so stdout stays the command's
//! output. Logs are off unless asked for.
//!
//! Log fields carry sizes, hashes, rule IDs, counts and timings. They never
//! carry input values, so `--debug` cannot leak customer records.

use clap::Args;
use serde_json::{Map, Value};

/// Logging flags shared by every product. Flatten them into the CLI with
/// `#[command(flatten)]`.
#[derive(Args, Clone, Copy, Debug, Default)]
pub struct LogOptions {
    /// Print what the command is doing to stderr
    #[arg(long, global = true)]
    pub verbose: bool,
    /// Also print internal details such as rule IDs and input fingerprints. Never input values
    #[arg(long, global = true)]
    pub debug: bool,
    /// Write log lines to stderr as JSON, one object per line
    #[arg(long, global = true)]
    pub json_logs: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Level {
    Info,
    Debug,
}

impl Level {
    fn label(self) -> &'static str {
        match self {
            Level::Info => "info",
            Level::Debug => "debug",
        }
    }
}

/// Writes log lines to stderr, or nothing when logging is off.
#[derive(Clone, Copy, Debug)]
pub struct Logger {
    /// Most detailed level to print. `None` turns logging off.
    max: Option<Level>,
    json: bool,
}

impl Logger {
    pub fn new(options: LogOptions) -> Self {
        let max = if options.debug {
            Some(Level::Debug)
        } else if options.verbose {
            Some(Level::Info)
        } else {
            None
        };
        Logger {
            max,
            json: options.json_logs,
        }
    }

    /// A step the command took, e.g. "policy loaded".
    pub fn info(&self, message: &str, fields: &[(&str, Value)]) {
        self.emit(Level::Info, message, fields);
    }

    /// Internal detail, shown only with `--debug`.
    pub fn debug(&self, message: &str, fields: &[(&str, Value)]) {
        self.emit(Level::Debug, message, fields);
    }

    fn emit(&self, level: Level, message: &str, fields: &[(&str, Value)]) {
        let Some(max) = self.max else {
            return;
        };
        if level > max {
            return;
        }
        if self.json {
            let mut object = Map::new();
            object.insert("level".into(), Value::from(level.label()));
            object.insert("message".into(), Value::from(message));
            let fields: Map<String, Value> = fields
                .iter()
                .map(|(key, value)| ((*key).to_string(), value.clone()))
                .collect();
            object.insert("fields".into(), Value::Object(fields));
            eprintln!("{}", Value::Object(object));
        } else {
            let mut line = format!("{}: {message}", level.label());
            for (key, value) in fields {
                line.push(' ');
                line.push_str(key);
                line.push('=');
                match value {
                    Value::String(text) => line.push_str(text),
                    other => line.push_str(&other.to_string()),
                }
            }
            eprintln!("{line}");
        }
    }
}
