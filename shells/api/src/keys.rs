//! `[keys]` entries such as `ctrl-r`, `alt-f` or `shift-tab`, parsed once and formatted per shell.

/// A key combination shellcraft can bind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Ctrl(char),
    Alt(char),
    ShiftTab,
}

/// Parse `ctrl-x`, `alt-x` (a single lowercase letter) or `shift-tab`, case-insensitively.
pub fn parse(key: &str) -> Option<Key> {
    let lower = key.to_ascii_lowercase();
    if lower == "shift-tab" {
        return Some(Key::ShiftTab);
    }
    let (kind, rest) = lower.split_once('-')?;
    let mut chars = rest.chars();
    let c = chars.next().filter(|c| c.is_ascii_lowercase())?;
    if chars.next().is_some() {
        return None;
    }
    match kind {
        "ctrl" => Some(Key::Ctrl(c)),
        "alt" => Some(Key::Alt(c)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_supported_forms() {
        assert_eq!(parse("ctrl-r"), Some(Key::Ctrl('r')));
        assert_eq!(parse("Alt-F"), Some(Key::Alt('f')));
        assert_eq!(parse("shift-tab"), Some(Key::ShiftTab));
        for bad in ["ctrl-1", "ctrl-ab", "super-x", "ctrl", "", "ctrl-"] {
            assert_eq!(parse(bad), None, "{bad}");
        }
    }
}
