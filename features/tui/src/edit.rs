//! Small, comment-preserving edits of `shellcraft.toml` text, and a line diff for the review screen.

/// Set `key = value` in `[section]` (created when missing), keeping everything else. `value` is a
/// TOML literal such as `true` or `"nord"`.
pub fn set_value(text: &str, section: &str, key: &str, value: &str) -> String {
    let header = format!("[{section}]");
    let line = format!("{key} = {value}");
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let Some(h) = lines.iter().position(|l| l.trim() == header) else {
        if lines.last().is_some_and(|l| !l.trim().is_empty()) {
            lines.push(String::new());
        }
        lines.push(header);
        lines.push(line);
        return lines.join("\n") + "\n";
    };
    let end = lines[h + 1..]
        .iter()
        .position(|l| l.trim_start().starts_with('['))
        .map_or(lines.len(), |i| h + 1 + i);
    let is_key = |l: &String| {
        let t = l.trim_start();
        t.strip_prefix(key)
            .is_some_and(|rest| rest.trim_start().starts_with('='))
    };
    match lines[h + 1..end].iter().position(is_key) {
        Some(i) => lines[h + 1 + i] = line,
        None => lines.insert(h + 1, line),
    }
    lines.join("\n") + "\n"
}

/// One line of a diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    Same(String),
    Removed(String),
    Added(String),
}

/// Line diff by longest common subsequence (configs are tiny).
pub fn diff(old: &str, new: &str) -> Vec<Change> {
    let (a, b): (Vec<&str>, Vec<&str>) = (old.lines().collect(), new.lines().collect());
    let mut lcs = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            lcs[i][j] = if a[i] == b[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }
    let (mut i, mut j, mut out) = (0, 0, Vec::new());
    while i < a.len() && j < b.len() {
        if a[i] == b[j] {
            out.push(Change::Same(a[i].to_string()));
            i += 1;
            j += 1;
        } else if lcs[i + 1][j] >= lcs[i][j + 1] {
            out.push(Change::Removed(a[i].to_string()));
            i += 1;
        } else {
            out.push(Change::Added(b[j].to_string()));
            j += 1;
        }
    }
    out.extend(a[i..].iter().map(|l| Change::Removed((*l).to_string())));
    out.extend(b[j..].iter().map(|l| Change::Added((*l).to_string())));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use shellcraft_core::Config;

    #[test]
    fn sets_existing_and_missing_keys_and_sections() {
        let text = "# my config\n[theme]\n# pick one\nname = \"nord\"\n[prompt]\nbuiltin = true\n";
        let t = set_value(text, "theme", "name", "\"dracula\"");
        assert!(
            t.contains("name = \"dracula\"")
                && t.contains("# pick one")
                && t.contains("# my config")
        );
        let t = set_value(&t, "prompt", "symbol", "\">\"");
        assert!(
            t.contains("[prompt]\nsymbol = \">\"\nbuiltin = true"),
            "{t}"
        );
        let t = set_value(&t, "plugin.docker", "enabled", "false");
        assert!(t.ends_with("[plugin.docker]\nenabled = false\n"), "{t}");
        assert!(Config::from_toml(&t).is_ok(), "{t}");
        // a key that merely starts the same is not the key
        let t = set_value("[a]\nnamespace = 1\n", "a", "name", "2");
        assert!(t.contains("name = 2") && t.contains("namespace = 1"));
    }

    #[test]
    fn diff_marks_changes_only() {
        let d = diff("a\nb\nc\n", "a\nB\nc\nd\n");
        assert_eq!(
            d,
            vec![
                Change::Same("a".into()),
                Change::Removed("b".into()),
                Change::Added("B".into()),
                Change::Same("c".into()),
                Change::Added("d".into()),
            ]
        );
        assert!(diff("x\n", "x\n")
            .iter()
            .all(|c| matches!(c, Change::Same(_))));
    }
}
