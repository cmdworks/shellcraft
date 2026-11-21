//! Marked block editing for shell rc files (`~/.zshrc`, `~/.bashrc`).

/// First line of the managed block.
pub const MARK_BEGIN: &str = "# >>> shellcraft >>>";
/// Last line of the managed block.
pub const MARK_END: &str = "# <<< shellcraft <<<";

fn block(body: &str) -> String {
    format!("{MARK_BEGIN}\n{}\n{MARK_END}\n", body.trim_end())
}

/// Byte range of an existing managed block, including its trailing newline.
fn find(rc: &str) -> Option<(usize, usize)> {
    let start = rc.find(MARK_BEGIN)?;
    let end_rel = rc[start..].find(MARK_END)?;
    let mut end = start + end_rel + MARK_END.len();
    if rc[end..].starts_with('\n') {
        end += 1;
    }
    Some((start, end))
}

/// Insert the block, or replace the existing one. Everything else is kept as is.
pub fn upsert(rc: &str, body: &str) -> String {
    let new = block(body);
    match find(rc) {
        Some((s, e)) => format!("{}{new}{}", &rc[..s], &rc[e..]),
        None if rc.is_empty() => new,
        None if rc.ends_with('\n') => format!("{rc}\n{new}"),
        None => format!("{rc}\n\n{new}"),
    }
}

/// Remove the managed block (and one blank line before it). Returns `None` if absent.
pub fn remove(rc: &str) -> Option<String> {
    let (s, e) = find(rc)?;
    let head = rc[..s]
        .strip_suffix("\n\n")
        .map_or(&rc[..s], |h| &rc[..h.len() + 1]);
    Some(format!("{head}{}", &rc[e..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_into_empty_and_existing() {
        assert_eq!(upsert("", "x"), format!("{MARK_BEGIN}\nx\n{MARK_END}\n"));
        let out = upsert("export A=1\n", "x");
        assert!(out.starts_with("export A=1\n\n# >>>"));
        let out = upsert("export A=1", "x");
        assert!(out.starts_with("export A=1\n\n# >>>"));
    }

    #[test]
    fn upsert_is_idempotent_and_replaces() {
        let once = upsert("a\n", "one");
        assert_eq!(upsert(&once, "one"), once);
        let twice = upsert(&once, "two");
        assert!(twice.contains("two") && !twice.contains("one"));
        assert_eq!(twice.matches(MARK_BEGIN).count(), 1);
    }

    #[test]
    fn remove_restores_original() {
        let original = "export A=1\n";
        let with = upsert(original, "x");
        assert_eq!(remove(&with).as_deref(), Some(original));
        assert_eq!(remove(original), None);
    }

    #[test]
    fn content_after_block_is_kept() {
        let rc = format!("a\n\n{MARK_BEGIN}\nold\n{MARK_END}\nb\n");
        let out = upsert(&rc, "new");
        assert!(out.starts_with("a\n\n# >>>") && out.ends_with("# <<< shellcraft <<<\nb\n"));
        assert_eq!(remove(&out).as_deref(), Some("a\nb\n"));
    }
}
