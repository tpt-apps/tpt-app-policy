//! Config discovery (spec §19): predictable folders under `./tpt`.
//!
//! Every folder is optional. The layout is plain files on disk, with no
//! hidden database.

use std::path::{Path, PathBuf};

/// The folder that holds the config layout, relative to the working directory.
pub const ROOT: &str = "tpt";

/// The subfolders of `./tpt` from spec §19.
pub const SUBDIRS: [&str; 5] = ["config", "policies", "schemas", "templates", "reports"];

/// The `tpt` folder under `base`, when it exists.
pub fn root_in(base: &Path) -> Option<PathBuf> {
    let root = base.join(ROOT);
    root.is_dir().then_some(root)
}

/// The subfolders of `root` that are missing. Empty when the layout is complete.
pub fn missing_subdirs(root: &Path) -> Vec<&'static str> {
    SUBDIRS
        .into_iter()
        .filter(|d| !root.join(d).is_dir())
        .collect()
}

/// Resolve a policy argument against `./tpt/policies`.
///
/// A path with a folder part, or one that exists as given, is used unchanged.
/// A bare name such as `expense` is looked up as `expense`, `expense.yaml`,
/// then `expense.yml` in `<base>/tpt/policies`. When nothing matches, the
/// argument is returned unchanged, so the error names what the user typed.
pub fn resolve_policy_in(base: &Path, arg: &Path) -> PathBuf {
    let is_bare_name = arg.components().count() == 1;
    if !is_bare_name || arg.exists() {
        return arg.to_path_buf();
    }
    let Some(name) = arg.to_str() else {
        return arg.to_path_buf();
    };
    let policies = base.join(ROOT).join("policies");
    for candidate in [
        name.to_string(),
        format!("{name}.yaml"),
        format!("{name}.yml"),
    ] {
        let path = policies.join(candidate);
        if path.is_file() {
            return path;
        }
    }
    arg.to_path_buf()
}

/// [`resolve_policy_in`] against the working directory.
pub fn resolve_policy(arg: &Path) -> PathBuf {
    resolve_policy_in(Path::new("."), arg)
}
