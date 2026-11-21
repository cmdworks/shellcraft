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

/// Parts a layout can place, left or right of the cursor line.
pub const LAYOUT_PARTS: &[&str] = &[
    "remote", "root", "venv", "aws", "dir", "segments", "duration", "status", "jobs", "symbol",
];

/// The shape of the prompt, for themes that want more than colours (`[theme.<name>.layout]`).
/// Every field is optional; a theme without a layout keeps shellcraft's own prompt.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Layout {
    /// Parts of the prompt, in order (see [`LAYOUT_PARTS`]); empty parts are skipped, the rest are
    /// joined with a space.
    pub left: Vec<String>,
    /// Parts shown at the right edge (zsh and fish), joined without a space.
    pub right: Vec<String>,
    /// Text before the directory (an icon, for example).
    pub dir_prefix: String,
    /// Text after the directory when it is not writable.
    pub dir_lock: String,
    /// The prompt symbol on success as `"<sgr>:<text>"` pieces, each in its own colour; empty means
    /// the configured symbol in the `ok` colour.
    pub symbol_ok: Vec<String>,
    /// The same after a failed command; `{status}` is the exit status; empty means the symbol in the `err` colour.
    pub symbol_err: Vec<String>,
    /// The warning shown as root, painted with `root_sgr`.
    pub root_text: String,
    pub root_sgr: String,
    /// Text before the Python/conda/node environment name and before the AWS profile.
    pub venv_prefix: String,
    pub aws_prefix: String,
    /// How the last command's duration (`{d}`) and the number of background jobs (`{n}`) look.
    pub duration_format: String,
    pub jobs_format: String,
    /// Show the duration from this many milliseconds on, instead of the default 2000 (only used
    /// while `[prompt] min_duration_ms` is left at its default).
    pub min_duration_ms: Option<u64>,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            left: ["remote", "dir", "segments", "duration", "status", "symbol"]
                .map(String::from)
                .to_vec(),
            right: Vec::new(),
            dir_prefix: String::new(),
            dir_lock: String::new(),
            symbol_ok: Vec::new(),
            symbol_err: Vec::new(),
            root_text: " ROOT ".into(),
            root_sgr: "41;37".into(),
            venv_prefix: String::new(),
            aws_prefix: String::new(),
            duration_format: "{d}".into(),
            jobs_format: "[{n}]".into(),
            min_duration_ms: None,
        }
    }
}

impl Layout {
    fn problem(&self) -> Option<String> {
        for p in self.left.iter().chain(&self.right) {
            if !LAYOUT_PARTS.contains(&p.as_str()) {
                return Some(format!(
                    "layout: unknown part `{p}` (parts: {})",
                    LAYOUT_PARTS.join(", ")
                ));
            }
        }
        for piece in self.symbol_ok.iter().chain(&self.symbol_err) {
            match piece.split_once(':') {
                Some((sgr, _)) if !sgr.is_empty() && sgr.chars().all(|c| c.is_ascii_digit() || c == ';') => {}
                _ => return Some(format!("layout: symbol piece `{piece}` must look like `31:❯` (SGR colour, colon, text)")),
            }
        }
        if self.root_sgr.is_empty()
            || !self
                .root_sgr
                .chars()
                .all(|c| c.is_ascii_digit() || c == ';')
        {
            return Some("layout: `root_sgr` must be SGR colour parameters like `41;37`".into());
        }
        let texts = [
            &self.dir_prefix,
            &self.dir_lock,
            &self.root_text,
            &self.venv_prefix,
            &self.aws_prefix,
            &self.duration_format,
            &self.jobs_format,
        ];
        if texts.iter().any(|t| t.chars().any(char::is_control))
            || self
                .symbol_ok
                .iter()
                .chain(&self.symbol_err)
                .any(|t| t.chars().any(char::is_control))
        {
            return Some("layout: texts must not contain control characters".into());
        }
        None
    }
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
    /// The shape of the prompt (`[theme.<name>.layout]`); absent keeps shellcraft's own prompt.
    pub layout: Option<Layout>,
    /// Defaults for plugin options while this theme is active (`[theme.<name>.options.<plugin>]`);
    /// the user's own `[plugin.<name>.options]` win.
    pub options: std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>>,
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
            layout: None,
            options: Default::default(),
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
        if let Some(why) = self.layout.as_ref().and_then(Layout::problem) {
            return Some(why);
        }
        for (plugin, opts) in &self.options {
            if plugin.is_empty() || opts.keys().any(String::is_empty) {
                return Some("options: plugin and option names must not be empty".into());
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
