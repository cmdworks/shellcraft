//! Quoting a string as one literal word, per shell.

use crate::ShellKind;

/// Quote `s` as one literal word that the given shell will not expand.
pub fn quote_for(kind: ShellKind, s: &str) -> String {
    match kind {
        ShellKind::Zsh | ShellKind::Bash => format!("'{}'", s.replace('\'', "'\\''")),
        ShellKind::Fish => format!("'{}'", s.replace('\\', "\\\\").replace('\'', "\\'")),
        ShellKind::Pwsh => format!("'{}'", s.replace('\'', "''")),
        ShellKind::Nushell => {
            // Raw string r#'...'# needs more `#` than any `'#...` run inside the text.
            let mut longest = 0;
            let mut run: Option<usize> = None;
            for c in s.chars() {
                match (c, run) {
                    ('\'', _) => run = Some(0),
                    ('#', Some(n)) => {
                        run = Some(n + 1);
                        longest = longest.max(n + 1);
                    }
                    _ => run = None,
                }
            }
            let hashes = "#".repeat(longest + 1);
            format!("r{hashes}'{s}'{hashes}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting() {
        assert_eq!(quote_for(ShellKind::Zsh, "it's $x"), r"'it'\''s $x'");
        assert_eq!(quote_for(ShellKind::Bash, "plain"), "'plain'");
        assert_eq!(quote_for(ShellKind::Fish, r"a\b'c"), r"'a\\b\'c'");
        assert_eq!(quote_for(ShellKind::Pwsh, "it's"), "'it''s'");
        assert_eq!(quote_for(ShellKind::Nushell, "plain"), "r#'plain'#");
        assert_eq!(quote_for(ShellKind::Nushell, "a'#b"), "r##'a'#b'##");
    }
}
