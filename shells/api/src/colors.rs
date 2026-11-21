//! Colour roles for the shells' own syntax highlighting, taken from the theme palette.

use serde::Serialize;
use shellcraft_core::theme::Palette;

/// One colour as the shells want it.
#[derive(Serialize)]
pub(crate) struct Role {
    /// SGR parameters, e.g. `38;5;110` (PowerShell, zsh).
    pub sgr: String,
    /// A fish colour: a name or `rrggbb`.
    pub fish: String,
}

/// Roles used by `fish_color_*` and PSReadLine colours.
#[derive(Serialize)]
pub(crate) struct Highlight {
    pub command: Role,
    pub error: Role,
    pub param: Role,
    pub quote: Role,
    pub operator: Role,
    pub comment: Role,
    pub redirection: Role,
}

fn role(sgr: &str) -> Role {
    Role {
        sgr: sgr.to_string(),
        fish: fish_color(sgr),
    }
}

pub(crate) fn highlight(p: &Palette) -> Highlight {
    Highlight {
        command: role(p.ok),
        error: role(p.err),
        param: role(p.version),
        quote: role(p.duration),
        operator: role(p.dir),
        comment: role(p.dim),
        redirection: role(p.tool),
    }
}

const NAMES: [&str; 8] = [
    "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white",
];

/// fish colour for SGR parameters `30..37`, `90..97` or `38;5;N`.
pub(crate) fn fish_color(sgr: &str) -> String {
    let n = match sgr.strip_prefix("38;5;") {
        Some(n) => n.parse::<u8>().ok(),
        None => sgr.parse::<u8>().ok().and_then(|c| match c {
            30..=37 => Some(c - 30),
            90..=97 => Some(c - 90 + 8),
            _ => None,
        }),
    };
    match n {
        Some(n @ 0..=7) => NAMES[n as usize].to_string(),
        Some(n @ 8..=15) => format!("br{}", NAMES[n as usize - 8]),
        Some(n @ 16..=231) => {
            let v = n - 16;
            let level = |l: u8| if l == 0 { 0 } else { 55 + 40 * l };
            format!(
                "{:02x}{:02x}{:02x}",
                level(v / 36),
                level(v / 6 % 6),
                level(v % 6)
            )
        }
        Some(n) => {
            let g = 8 + 10 * (n - 232);
            format!("{g:02x}{g:02x}{g:02x}")
        }
        None => "normal".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_sgr_to_fish() {
        assert_eq!(fish_color("34"), "blue");
        assert_eq!(fish_color("94"), "brblue");
        assert_eq!(fish_color("90"), "brblack");
        assert_eq!(fish_color("38;5;110"), "87afd7");
        assert_eq!(fish_color("38;5;245"), "8a8a8a");
        assert_eq!(fish_color("1;31"), "normal");
    }
}
