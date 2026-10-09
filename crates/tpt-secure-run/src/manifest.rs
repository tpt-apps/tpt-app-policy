//! The permission manifest (YAML).
//!
//! A script has no authority unless the manifest grants it. Only file access
//! is supported in this version. Keys for network, environment and
//! subprocess access are recognised so that they are refused with a clear
//! message, rather than silently ignored.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Largest file a script may read, or write, in one run.
pub const MAX_FILE_BYTES: usize = 10 * 1024 * 1024;

const DEFAULT_MAX_STEPS: u64 = 10_000_000;
const DEFAULT_MAX_MEMORY_PAGES: u64 = 16;
const DEFAULT_MAX_CALL_DEPTH: usize = 256;

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// Name of the script, shown in the report.
    pub name: String,
    #[serde(default)]
    pub resources: Resources,
    #[serde(default)]
    pub limits: Limits,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Resources {
    /// Files the script may read. The script refers to them by position (0, 1, ...).
    #[serde(default)]
    pub fs_read: Vec<FileGrant>,
    /// Files the script may write. The script refers to them by position.
    #[serde(default)]
    pub fs_write: Vec<FileGrant>,
    // Recognised only so that they can be refused with a clear message.
    #[serde(default)]
    pub net_connect: Option<serde_yaml::Value>,
    #[serde(default)]
    pub process_spawn: Option<serde_yaml::Value>,
    #[serde(default)]
    pub env: Option<serde_yaml::Value>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileGrant {
    /// Label used in the report. Not visible to the script.
    pub name: String,
    /// Path to the file. Relative paths are taken from the manifest's folder.
    pub path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Limits {
    /// Instructions the script may execute before it is stopped.
    pub max_steps: u64,
    /// Linear memory, in 64 KiB pages. 16 pages is 1 MiB.
    pub max_memory_pages: u64,
    /// Nested call depth.
    pub max_call_depth: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_steps: DEFAULT_MAX_STEPS,
            max_memory_pages: DEFAULT_MAX_MEMORY_PAGES,
            max_call_depth: DEFAULT_MAX_CALL_DEPTH,
        }
    }
}

/// Parse and validate a manifest. Errors name the problem and how to fix it.
pub fn parse_manifest(text: &str) -> Result<Manifest, String> {
    let manifest: Manifest =
        serde_yaml::from_str(text).map_err(|e| format!("invalid manifest: {e}"))?;
    validate(&manifest)?;
    Ok(manifest)
}

fn validate(m: &Manifest) -> Result<(), String> {
    if m.name.trim().is_empty() {
        return Err("manifest 'name' is empty. Fix: give the script a name".into());
    }
    let r = &m.resources;
    for (key, value) in [
        ("net_connect", &r.net_connect),
        ("process_spawn", &r.process_spawn),
        ("env", &r.env),
    ] {
        if value.is_some() {
            return Err(format!(
                "'{key}' is not supported in this version, so no script can use it. \
                 Fix: remove the '{key}' entry. Only fs_read and fs_write are granted"
            ));
        }
    }
    check_grants("fs_read", &r.fs_read)?;
    check_grants("fs_write", &r.fs_write)?;
    let l = &m.limits;
    if l.max_steps == 0 {
        return Err("limits.max_steps must be at least 1".into());
    }
    if !(1..=256).contains(&l.max_memory_pages) {
        return Err("limits.max_memory_pages must be from 1 to 256 (64 KiB each)".into());
    }
    if !(1..=10_000).contains(&l.max_call_depth) {
        return Err("limits.max_call_depth must be from 1 to 10000".into());
    }
    Ok(())
}

fn check_grants(kind: &str, grants: &[FileGrant]) -> Result<(), String> {
    let mut names = HashSet::new();
    for g in grants {
        if g.name.trim().is_empty() {
            return Err(format!(
                "{kind} entry has an empty 'name'. Fix: name the file"
            ));
        }
        if !names.insert(g.name.as_str()) {
            return Err(format!(
                "{kind} has two entries named '{}'. Fix: give each file a unique name",
                g.name
            ));
        }
        if g.path.as_os_str().is_empty() {
            return Err(format!("{kind} entry '{}' has an empty 'path'", g.name));
        }
    }
    Ok(())
}

/// Resolve a grant's path against the manifest's folder.
pub fn resolve(base_dir: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base_dir.join(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_manifest_grants_nothing_and_uses_default_limits() {
        let m = parse_manifest("name: demo\n").unwrap();
        assert!(m.resources.fs_read.is_empty());
        assert!(m.resources.fs_write.is_empty());
        assert_eq!(m.limits, Limits::default());
    }

    #[test]
    fn network_process_and_env_are_refused_not_ignored() {
        for key in ["net_connect", "process_spawn", "env"] {
            let text = format!("name: demo\nresources:\n  {key}: [\"x\"]\n");
            let err = parse_manifest(&text).unwrap_err();
            assert!(err.contains(key) && err.contains("not supported"), "{err}");
        }
    }

    #[test]
    fn unknown_keys_are_rejected() {
        let err = parse_manifest("name: demo\nresource: {}\n").unwrap_err();
        assert!(err.contains("resource"), "{err}");
    }

    #[test]
    fn duplicate_grant_names_are_rejected() {
        let text = "name: demo\nresources:\n  fs_read:\n    - {name: a, path: x}\n    - {name: a, path: y}\n";
        assert!(parse_manifest(text).unwrap_err().contains("unique name"));
    }

    #[test]
    fn limits_out_of_range_are_rejected() {
        let err = parse_manifest("name: demo\nlimits:\n  max_steps: 0\n").unwrap_err();
        assert!(err.contains("max_steps"), "{err}");
        let err = parse_manifest("name: demo\nlimits:\n  max_memory_pages: 1000\n").unwrap_err();
        assert!(err.contains("max_memory_pages"), "{err}");
    }
}
