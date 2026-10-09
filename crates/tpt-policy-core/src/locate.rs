//! Finds the line of a semantic policy error from its key path.
//!
//! The parser works on YAML values, which do not keep positions. So the line
//! is read back from the text, using the same path the error reports, e.g.
//! `rules[0].when.amount.gt`. This follows block-style YAML: indentation and
//! `- ` items. Flow-style values (`{...}`, `[...]`) are not followed, so for
//! those the line of the nearest key that was found is returned.

/// One step in an error path: a mapping key, or a list index.
#[derive(Debug, PartialEq, Eq)]
enum Seg {
    Key(String),
    Index(usize),
}

/// A line of YAML that is not blank or a comment.
struct Line<'a> {
    /// 1-based line number.
    no: usize,
    /// Leading spaces.
    indent: usize,
    /// The line starts a `- ` list item.
    dash: bool,
    /// Column where the item's content starts. For a `- ` line, that is past
    /// the dash, so `- id: x` gives the column of `id`. Otherwise it is `indent`.
    content_col: usize,
    /// The content, with any `- ` removed.
    text: &'a str,
}

/// The line of the error at `location`, or `None` if it has no path.
///
/// If the path does not exist in the text (for example a missing key), the
/// line of the deepest part of the path that does exist is returned.
pub(crate) fn line_of(source: &str, location: &str) -> Option<usize> {
    let path = parse_path(location)?;
    let lines = significant_lines(source);
    let mut best: Option<usize> = None;
    let mut cursor = 0usize;
    let mut col = lines.first()?.indent;
    // Set after a key matches: its children are searched at the next level down.
    let mut parent_col: Option<usize> = None;
    let mut k = 0usize;

    while k < path.len() {
        if let Some(parent) = parent_col.take() {
            match child_col(&lines, cursor, parent) {
                Some(c) => col = c,
                None => break,
            }
        }
        match &path[k] {
            Seg::Key(_) => {
                // A key can contain dots (`expense.amount`), so try the longest
                // run of keys first.
                let mut matched = None;
                for end in (k + 1..=path.len()).rev() {
                    let keys: Option<Vec<&str>> = path[k..end]
                        .iter()
                        .map(|seg| match seg {
                            Seg::Key(name) => Some(name.as_str()),
                            Seg::Index(_) => None,
                        })
                        .collect();
                    let Some(keys) = keys else { continue };
                    if let Some(idx) = find_key(&lines, cursor, col, &keys.join(".")) {
                        matched = Some((idx, end));
                        break;
                    }
                }
                let Some((idx, end)) = matched else { break };
                cursor = idx;
                best = Some(idx);
                parent_col = Some(lines[idx].content_col);
                k = end;
            }
            Seg::Index(n) => {
                let Some(idx) = find_item(&lines, cursor, col, *n) else {
                    break;
                };
                cursor = idx;
                best = Some(idx);
                col = lines[idx].content_col;
                k += 1;
            }
        }
    }
    best.map(|idx| lines[idx].no)
}

/// Read a path such as `rules[0].when.amount.gt` into segments. Returns `None`
/// for `<root>`, an empty path, or text that is not a path.
fn parse_path(location: &str) -> Option<Vec<Seg>> {
    if location.is_empty() || location == "<root>" {
        return None;
    }
    let mut segs = Vec::new();
    for part in location.split('.') {
        let (name, mut rest) = match part.find('[') {
            Some(i) => (&part[..i], &part[i..]),
            None => (part, ""),
        };
        if !name.is_empty() {
            segs.push(Seg::Key(name.to_string()));
        }
        while let Some(after) = rest.strip_prefix('[') {
            let end = after.find(']')?;
            let n: usize = after[..end].parse().ok()?;
            segs.push(Seg::Index(n));
            rest = &after[end + 1..];
        }
        if !rest.is_empty() {
            return None;
        }
    }
    (!segs.is_empty()).then_some(segs)
}

/// Lines that hold content: not blank, not comments, not document markers.
fn significant_lines(source: &str) -> Vec<Line<'_>> {
    let mut lines = Vec::new();
    for (i, raw) in source.lines().enumerate() {
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed == "---" || trimmed == "..." {
            continue;
        }
        let indent = raw.len() - raw.trim_start_matches(' ').len();
        let rest = &raw[indent..];
        let (dash, content_col, text) = if rest == "-" {
            (true, indent + 1, "")
        } else if let Some(after) = rest.strip_prefix("- ") {
            let spaces = after.len() - after.trim_start_matches(' ').len();
            (true, indent + 2 + spaces, after.trim_start_matches(' '))
        } else {
            (false, indent, rest)
        };
        lines.push(Line {
            no: i + 1,
            indent,
            dash,
            content_col,
            text,
        });
    }
    lines
}

/// The column of the children of a key at `parent`, found from the line after
/// the key. Children are either deeper, or a list at the key's own column.
fn child_col(lines: &[Line], key_line: usize, parent: usize) -> Option<usize> {
    let next = lines.get(key_line + 1)?;
    if next.indent > parent || (next.indent == parent && next.dash) {
        Some(next.indent)
    } else {
        None
    }
}

/// The index of the line that holds `key` at column `col`, searching from
/// `from`. Stops when the search leaves the current level.
fn find_key(lines: &[Line], from: usize, col: usize, key: &str) -> Option<usize> {
    for (idx, line) in lines.iter().enumerate().skip(from) {
        if idx != from && line.indent < col {
            return None;
        }
        let at_level = if line.dash {
            line.content_col == col
        } else {
            line.indent == col
        };
        if at_level && key_in(line.text) == Some(key) {
            return Some(idx);
        }
    }
    None
}

/// The index of the `n`th list item at column `col`, searching after `from`.
fn find_item(lines: &[Line], from: usize, col: usize, n: usize) -> Option<usize> {
    let mut count = 0;
    for (idx, line) in lines.iter().enumerate().skip(from + 1) {
        if line.indent < col {
            return None;
        }
        if line.indent == col {
            if !line.dash {
                // A key at the list's level: the list has ended.
                return None;
            }
            if count == n {
                return Some(idx);
            }
            count += 1;
        }
    }
    None
}

/// The key on a line of the form `key: value` or `"key": value`.
fn key_in(text: &str) -> Option<&str> {
    if let Some(quote) = text.chars().next().filter(|c| *c == '"' || *c == '\'') {
        let rest = &text[1..];
        let end = rest.find(quote)?;
        return rest[end + 1..].starts_with(':').then(|| &rest[..end]);
    }
    let bytes = text.as_bytes();
    for (i, b) in bytes.iter().enumerate() {
        if *b == b':' && (i + 1 == bytes.len() || bytes[i + 1] == b' ') {
            return Some(text[..i].trim_end());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const POLICY: &str = "\
policy: expenses
version: \"1.0\"
rules:
  - id: big
    when:
      amount:
        gt: 5000
    then:
      decision: review
  - id: small
    when:
      expense.category:
        equals: travel
    then:
      decision: approved
";

    #[test]
    fn finds_list_items_and_nested_keys() {
        assert_eq!(line_of(POLICY, "rules[0]"), Some(4));
        assert_eq!(line_of(POLICY, "rules[0].id"), Some(4));
        assert_eq!(line_of(POLICY, "rules[0].when.amount.gt"), Some(7));
        assert_eq!(line_of(POLICY, "rules[0].then.decision"), Some(9));
        assert_eq!(line_of(POLICY, "rules[1].id"), Some(10));
    }

    #[test]
    fn finds_top_level_keys() {
        assert_eq!(line_of(POLICY, "version"), Some(2));
        assert_eq!(line_of(POLICY, "rules"), Some(3));
    }

    #[test]
    fn keys_with_dots_are_matched_whole() {
        assert_eq!(line_of(POLICY, "rules[1].when.expense.category"), Some(12));
    }

    #[test]
    fn missing_keys_give_the_deepest_line_that_exists() {
        // No `warning` key under then, so the `then` line is used.
        assert_eq!(line_of(POLICY, "rules[0].then.warning"), Some(8));
        // No sixth rule, so the `rules` line is used.
        assert_eq!(line_of(POLICY, "rules[5].id"), Some(3));
    }

    #[test]
    fn root_and_empty_paths_have_no_line() {
        assert_eq!(line_of(POLICY, "<root>"), None);
        assert_eq!(line_of(POLICY, ""), None);
    }

    #[test]
    fn sequence_items_with_inline_keys_and_comments() {
        let src = "\
rules:
# a comment
- id: first   # trailing
  then:
    decision: review
- id: second
";
        assert_eq!(line_of(src, "rules[1].id"), Some(6));
        assert_eq!(line_of(src, "rules[0].then.decision"), Some(5));
    }

    #[test]
    fn flow_style_values_fall_back_to_the_enclosing_key() {
        let src = "rules:\n  - id: x\n    when: {amount: {gt: 5}}\n";
        assert_eq!(line_of(src, "rules[0].when.amount.gt"), Some(3));
    }

    #[test]
    fn parse_path_reads_indexes_and_keys() {
        assert_eq!(
            parse_path("rules[2].when.all[0]"),
            Some(vec![
                Seg::Key("rules".into()),
                Seg::Index(2),
                Seg::Key("when".into()),
                Seg::Key("all".into()),
                Seg::Index(0),
            ])
        );
        assert_eq!(
            parse_path("line 4, column 3"),
            Some(vec![Seg::Key("line 4, column 3".into())])
        );
        assert_eq!(parse_path("rules[x]"), None);
    }
}
