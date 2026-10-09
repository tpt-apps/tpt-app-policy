//! Host functions: the only way a script can reach the outside world.
//!
//! A script imports functions from the module `tpt`. Each import is linked
//! only when the manifest grants the authority it needs. Anything else is
//! refused before the script runs.
//!
//! Scripts pass integers only. A host function cannot read guest memory, so
//! file names never come from the script. A script names a file by its
//! position in the manifest (its handle), and the path comes from the manifest.
//!
//! File access goes through a `tpt-capsec` token minted from the run's root
//! capability, scoped to the granted path.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tpt_capsec_core::RootCapability;
use tpt_wasm_runtime::HostFunction;
use tpt_wasm_types::{FunctionType, ResultType, Value, ValueType};

use crate::manifest::MAX_FILE_BYTES;

/// Module name every host import is looked up under.
pub const MODULE: &str = "tpt";

/// Most files a script may have open at once.
const MAX_OPEN_FILES: usize = 16;
/// Most log entries kept from one run.
const MAX_LOG_ENTRIES: usize = 10_000;

/// A host import the runner can offer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    /// `log(value: i64)`. Records a value in the run log. Needs no grant.
    Log,
    /// `fs_open_read(handle: i32) -> i32`. Returns a file descriptor, or -1.
    OpenRead,
    /// `fs_size(fd: i32) -> i64`. Returns the file's length in bytes, or -1.
    Size,
    /// `fs_byte(fd: i32, offset: i64) -> i32`. Returns one byte, or -1.
    Byte,
    /// `fs_write_byte(handle: i32, byte: i32) -> i32`. Returns 0, or -1.
    WriteByte,
}

impl Op {
    pub const ALL: [Op; 5] = [Op::Log, Op::OpenRead, Op::Size, Op::Byte, Op::WriteByte];

    pub fn name(self) -> &'static str {
        match self {
            Op::Log => "log",
            Op::OpenRead => "fs_open_read",
            Op::Size => "fs_size",
            Op::Byte => "fs_byte",
            Op::WriteByte => "fs_write_byte",
        }
    }

    /// The manifest section that grants this import, or `None` if it needs no grant.
    pub fn grant(self) -> Option<&'static str> {
        match self {
            Op::Log => None,
            Op::OpenRead | Op::Size | Op::Byte => Some("fs_read"),
            Op::WriteByte => Some("fs_write"),
        }
    }

    fn function_type(self) -> FunctionType {
        use ValueType::{I32, I64};
        let (params, results) = match self {
            Op::Log => (vec![I64], vec![]),
            Op::OpenRead => (vec![I32], vec![I32]),
            Op::Size => (vec![I32], vec![I64]),
            Op::Byte => (vec![I32, I64], vec![I32]),
            Op::WriteByte => (vec![I32, I32], vec![I32]),
        };
        FunctionType {
            params: ResultType(params),
            results: ResultType(results),
        }
    }
}

/// Everything a run collected from its host functions.
#[derive(Debug, Default)]
pub struct Collected {
    pub log: Vec<i64>,
    pub denials: Vec<String>,
    pub read_bytes: u64,
    pub write_bytes: u64,
    /// Output bytes by write handle (manifest position).
    pub outputs: BTreeMap<usize, Vec<u8>>,
}

#[derive(Default)]
struct State {
    files: Vec<Vec<u8>>,
    collected: Collected,
}

/// State shared by the host functions of one run.
pub struct Shared {
    root: RootCapability,
    reads: Vec<PathBuf>,
    writes: Vec<PathBuf>,
    state: Mutex<State>,
}

impl Shared {
    /// `reads` and `writes` are the granted paths, in manifest order.
    pub fn new(reads: Vec<PathBuf>, writes: Vec<PathBuf>) -> Arc<Self> {
        Arc::new(Self {
            root: RootCapability::acquire(),
            reads,
            writes,
            state: Mutex::new(State::default()),
        })
    }

    /// The host imports this run offers, given its grants.
    pub fn offered(&self) -> Vec<Op> {
        Op::ALL
            .into_iter()
            .filter(|op| match op.grant() {
                None => true,
                Some("fs_read") => !self.reads.is_empty(),
                Some(_) => !self.writes.is_empty(),
            })
            .collect()
    }

    /// Take what the run collected. Call once, after the script has finished.
    pub fn collect(&self) -> Collected {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        std::mem::take(&mut state.collected)
    }

    fn with_state<T>(&self, f: impl FnOnce(&mut State) -> T) -> T {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        f(&mut state)
    }

    fn deny(&self, why: String) {
        self.with_state(|s| s.collected.denials.push(why));
    }

    fn open_read(&self, handle: i32) -> i32 {
        let Some(scope) = usize::try_from(handle).ok().and_then(|h| self.reads.get(h)) else {
            self.deny(format!("fs_open_read: handle {handle} is not granted"));
            return -1;
        };
        if self.with_state(|s| s.files.len()) >= MAX_OPEN_FILES {
            self.deny(format!(
                "fs_open_read: more than {MAX_OPEN_FILES} files open"
            ));
            return -1;
        }
        // The token is the authority for this read. It is scoped to the granted path.
        let token = self.root.delegate_fs_read(scope);
        match read_capped(token.scope()) {
            Ok(bytes) => self.with_state(|s| {
                s.collected.read_bytes += bytes.len() as u64;
                s.files.push(bytes);
                i32::try_from(s.files.len() - 1).unwrap_or(-1)
            }),
            Err(e) => {
                self.deny(format!(
                    "fs_open_read: handle {handle} could not be read: {e}"
                ));
                -1
            }
        }
    }

    fn size(&self, fd: i32) -> i64 {
        self.with_state(|s| {
            usize::try_from(fd)
                .ok()
                .and_then(|fd| s.files.get(fd))
                .map_or(-1, |bytes| bytes.len() as i64)
        })
    }

    fn byte(&self, fd: i32, offset: i64) -> i32 {
        self.with_state(|s| {
            usize::try_from(fd)
                .ok()
                .and_then(|fd| s.files.get(fd))
                .and_then(|bytes| usize::try_from(offset).ok().and_then(|o| bytes.get(o)))
                .map_or(-1, |b| i32::from(*b))
        })
    }

    fn write_byte(&self, handle: i32, byte: i32) -> i32 {
        let Some(handle) = usize::try_from(handle)
            .ok()
            .filter(|h| *h < self.writes.len())
        else {
            self.deny(format!("fs_write_byte: handle {handle} is not granted"));
            return -1;
        };
        self.with_state(|s| {
            let out = s.collected.outputs.entry(handle).or_default();
            if out.len() >= MAX_FILE_BYTES {
                return -1;
            }
            out.push(byte as u8);
            s.collected.write_bytes += 1;
            0
        })
    }

    fn log(&self, value: i64) {
        self.with_state(|s| {
            if s.collected.log.len() < MAX_LOG_ENTRIES {
                s.collected.log.push(value);
            }
        });
    }
}

fn read_capped(path: &Path) -> io::Result<Vec<u8>> {
    let len = fs::metadata(path)?.len();
    if len > MAX_FILE_BYTES as u64 {
        return Err(io::Error::other(format!(
            "file is {len} bytes; the limit is {MAX_FILE_BYTES}"
        )));
    }
    fs::read(path)
}

/// One host import, bound to the run's shared state.
pub struct Import {
    shared: Arc<Shared>,
    op: Op,
}

impl Import {
    pub fn new(shared: &Arc<Shared>, op: Op) -> Self {
        Self {
            shared: Arc::clone(shared),
            op,
        }
    }
}

impl HostFunction for Import {
    fn function_type(&self) -> FunctionType {
        self.op.function_type()
    }

    fn call(&self, args: &[Value]) -> Result<Vec<Value>, String> {
        let s = &self.shared;
        let i32_at = |i: usize| match args.get(i) {
            Some(Value::I32(v)) => Ok(*v),
            _ => Err(format!("{}: argument {i} must be i32", self.op.name())),
        };
        let i64_at = |i: usize| match args.get(i) {
            Some(Value::I64(v)) => Ok(*v),
            _ => Err(format!("{}: argument {i} must be i64", self.op.name())),
        };
        Ok(match self.op {
            Op::Log => {
                s.log(i64_at(0)?);
                vec![]
            }
            Op::OpenRead => vec![Value::I32(s.open_read(i32_at(0)?))],
            Op::Size => vec![Value::I64(s.size(i32_at(0)?))],
            Op::Byte => vec![Value::I32(s.byte(i32_at(0)?, i64_at(1)?))],
            Op::WriteByte => vec![Value::I32(s.write_byte(i32_at(0)?, i32_at(1)?))],
        })
    }
}
