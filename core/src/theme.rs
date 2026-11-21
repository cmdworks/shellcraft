//! Built-in colour themes. A palette recolours the prompt and sets fzf and bat colours.

/// SGR parameters (e.g. `38;5;110`) for prompt segments, plus tool settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct Palette {
    pub dir: &'static str,
    pub git: &'static str,
    pub duration: &'static str,
    pub ok: &'static str,
    pub err: &'static str,
    pub dim: &'static str,
    /// Language and tool versions.
    pub version: &'static str,
    /// Tool and environment markers (docker, ...).
    pub tool: &'static str,
    /// `FZF_DEFAULT_OPTS` value, empty to leave fzf alone.
    pub fzf: &'static str,
    /// `BAT_THEME` value, empty to leave bat alone.
    pub bat: &'static str,
}

impl Palette {
    /// Plain ANSI colours, no tool settings.
    pub const DEFAULT: Palette = Palette {
        dir: "34",
        git: "35",
        duration: "33",
        ok: "32",
        err: "31",
        dim: "90",
        version: "36",
        tool: "94",
        fzf: "",
        bat: "",
    };
}

/// A theme as plugins and the config carry it: owned strings, every field optional (a missing
/// colour is the default theme's).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ThemeColors {
    pub dir: String,
    pub git: String,
    pub duration: String,
    pub ok: String,
    pub err: String,
    pub dim: String,
    pub version: String,
    pub tool: String,
    pub fzf: String,
    pub bat: String,
}

impl Default for ThemeColors {
    fn default() -> Self {
        let d = Palette::DEFAULT;
        Self {
            dir: d.dir.into(),
            git: d.git.into(),
            duration: d.duration.into(),
            ok: d.ok.into(),
            err: d.err.into(),
            dim: d.dim.into(),
            version: d.version.into(),
            tool: d.tool.into(),
            fzf: d.fzf.into(),
            bat: d.bat.into(),
        }
    }
}

impl ThemeColors {
    /// A palette of this theme. The strings are leaked: palettes live as long as the short-lived
    /// process that asked for one.
    pub fn palette(&self) -> Palette {
        let leak = |s: &str| -> &'static str { Box::leak(s.to_string().into_boxed_str()) };
        Palette {
            dir: leak(&self.dir),
            git: leak(&self.git),
            duration: leak(&self.duration),
            ok: leak(&self.ok),
            err: leak(&self.err),
            dim: leak(&self.dim),
            version: leak(&self.version),
            tool: leak(&self.tool),
            fzf: leak(&self.fzf),
            bat: leak(&self.bat),
        }
    }

    /// Why these colours cannot be used, if they cannot: SGR parameters are digits and `;`.
    pub fn problem(&self) -> Option<String> {
        for (k, v) in [
            ("dir", &self.dir),
            ("git", &self.git),
            ("duration", &self.duration),
            ("ok", &self.ok),
            ("err", &self.err),
            ("dim", &self.dim),
            ("version", &self.version),
            ("tool", &self.tool),
        ] {
            if v.is_empty() || !v.chars().all(|c| c.is_ascii_digit() || c == ';') {
                return Some(format!(
                    "`{k}` must be SGR colour parameters like `34` or `38;5;110`"
                ));
            }
        }
        for (k, v) in [("fzf", &self.fzf), ("bat", &self.bat)] {
            if v.chars().any(char::is_control) {
                return Some(format!("`{k}` must not contain control characters"));
            }
        }
        None
    }
}

/// The only theme in the binary; the rest come from theme plugins (`[theme.<name>]`).
pub const NAMES: &[&str] = &["default"];

/// Palette for a built-in theme name.
pub fn palette(name: &str) -> Option<Palette> {
    (name == "default").then_some(Palette::DEFAULT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_listed_name_has_a_palette() {
        for n in NAMES {
            assert!(palette(n).is_some(), "{n}");
        }
        assert!(palette("nope").is_none());
    }

    #[test]
    fn theme_colours_default_fill_and_validate() {
        let t: ThemeColors = toml::from_str("dir = \"38;5;110\"").expect("parse");
        assert_eq!((t.dir.as_str(), t.git.as_str()), ("38;5;110", "35"));
        assert_eq!(t.palette().dir, "38;5;110");
        assert!(t.problem().is_none());
        let bad = ThemeColors {
            ok: "green".into(),
            ..ThemeColors::default()
        };
        assert!(bad.problem().expect("problem").contains("`ok`"));
    }
}
