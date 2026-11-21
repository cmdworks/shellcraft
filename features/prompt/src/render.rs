//! Prompt renderer: `[user@host ]dir [branch] [took 3s] ❯ `.
//!
//! Pure string building; everything after the directory comes from plugin segments
//! (`shellcraft-plugin`), so a render stays far below the 5 ms budget.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use shellcraft_core::theme::{Layout, Palette};
use shellcraft_core::{PluginSettings, PromptSection, Shell};
use shellcraft_plugin::{Ctx, Tone};

/// Everything the prompt needs, collected by the caller.
#[derive(Debug, Default, Clone)]
pub struct Input {
    pub cwd: PathBuf,
    pub home: Option<PathBuf>,
    /// Exit status of the last command.
    pub status: i32,
    pub duration_ms: u64,
    /// `user@host`, set only in SSH sessions.
    pub remote: Option<String>,
    /// shellcraft cache directory, for plugin caches.
    pub cache_dir: Option<PathBuf>,
    /// The shell runs as root; the current directory is not writable; number of background jobs.
    /// The shell passes these (only themes with a layout show them).
    pub root: bool,
    pub readonly: bool,
    pub jobs: u32,
    /// The few environment variables a layout reads (`VIRTUAL_ENV`, `CONDA_DEFAULT_ENV`, `NVM_BIN`,
    /// `AWS_PROFILE`, `AWS_DEFAULT_PROFILE`), collected by the caller.
    pub env: BTreeMap<String, String>,
}

/// The variables [`Input::env`] holds.
pub const LAYOUT_ENV: &[&str] = &[
    "VIRTUAL_ENV",
    "CONDA_DEFAULT_ENV",
    "NVM_BIN",
    "AWS_PROFILE",
    "AWS_DEFAULT_PROFILE",
];

fn tone_code(pal: &Palette, tone: Tone) -> &'static str {
    match tone {
        Tone::Git => pal.git,
        Tone::Version => pal.version,
        Tone::Tool => pal.tool,
        Tone::Warn => pal.duration,
        Tone::Dim => pal.dim,
        Tone::Ok => pal.ok,
        Tone::Err => pal.err,
    }
}

/// `~`-abbreviated path keeping the last three components.
pub fn short_dir(cwd: &Path, home: Option<&Path>) -> String {
    let (prefix, rest) = match home.and_then(|h| cwd.strip_prefix(h).ok()) {
        Some(r) => ("~".to_string(), r.to_path_buf()),
        None => (String::new(), cwd.to_path_buf()),
    };
    let parts: Vec<String> = rest
        .components()
        .filter_map(|c| match c {
            std::path::Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    let (lead, tail) = if parts.len() > 3 {
        ("\u{2026}/".to_string(), &parts[parts.len() - 3..])
    } else if prefix.is_empty() {
        ("/".to_string(), &parts[..])
    } else {
        (format!("{prefix}/"), &parts[..])
    };
    if parts.is_empty() {
        return if prefix.is_empty() {
            "/".into()
        } else {
            prefix
        };
    }
    format!("{lead}{}", tail.join("/"))
}

/// `1h2m`, `3m4s`, `5s` or `1.5s`-style text for a duration.
pub fn human_duration(ms: u64) -> String {
    let s = ms / 1000;
    match s {
        0 => format!("{ms}ms"),
        1..=59 => format!("{s}s"),
        60..=3599 => format!("{}m{}s", s / 60, s % 60),
        _ => format!("{}h{}m", s / 3600, (s % 3600) / 60),
    }
}

/// Escape text so the shell prints it literally inside a prompt string.
fn literal(shell: Shell, text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match (shell, c) {
            (Shell::Zsh, '%') => out.push_str("%%"),
            (Shell::Bash, '\\' | '$' | '`') => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

/// Colour `text` with an SGR code, wrapped so the shell does not count it toward the width.
fn paint(shell: Shell, code: &str, text: &str) -> String {
    let (open, close) = match shell {
        Shell::Zsh => ("%{\u{1b}[", "%}"),
        Shell::Bash => ("\\[\u{1b}[", "\\]"),
        _ => ("\u{1b}[", ""),
    };
    let reset = match shell {
        Shell::Zsh => "%{\u{1b}[0m%}",
        Shell::Bash => "\\[\u{1b}[0m\\]",
        _ => "\u{1b}[0m",
    };
    format!("{open}{code}m{close}{}{reset}", literal(shell, text))
}

/// Render the prompt for `shell`; a theme with a layout draws it through [`render_layout`].
pub fn render(
    shell: Shell,
    cfg: &PromptSection,
    plugins: &BTreeMap<String, PluginSettings>,
    pal: &Palette,
    layout: Option<&Layout>,
    input: &Input,
) -> String {
    if let Some(l) = layout {
        return render_layout(shell, cfg, plugins, pal, l, input).0;
    }
    let mut out = String::new();
    if let Some(r) = &input.remote {
        out.push_str(&paint(shell, pal.dim, r));
        out.push(' ');
    }
    out.push_str(&paint(
        shell,
        pal.dir,
        &short_dir(&input.cwd, input.home.as_deref()),
    ));
    out.push_str(&segments(shell, cfg, plugins, pal, input));
    if cfg.min_duration_ms > 0 && input.duration_ms >= cfg.min_duration_ms {
        out.push(' ');
        out.push_str(&paint(
            shell,
            pal.duration,
            &human_duration(input.duration_ms),
        ));
    }
    if input.status != 0 {
        out.push(' ');
        out.push_str(&paint(shell, pal.err, &format!("[{}]", input.status)));
    }
    out.push(' ');
    let color = if input.status == 0 { pal.ok } else { pal.err };
    out.push_str(&paint(shell, color, &cfg.symbol));
    out.push(' ');
    out
}

/// The environment names a layout's `venv` part shows: Python virtualenv, conda, node (nvm).
fn env_names(env: &BTreeMap<String, String>) -> Vec<String> {
    let var = |k: &str| env.get(k).filter(|v| !v.is_empty());
    let last = |p: &Path| p.file_name().map(|n| n.to_string_lossy().into_owned());
    let mut out = Vec::new();
    if let Some(v) = var("VIRTUAL_ENV").and_then(|p| last(Path::new(p))) {
        out.push(v);
    } else if let Some(c) = var("CONDA_DEFAULT_ENV") {
        out.push(c.clone());
    }
    // NVM_BIN is `<root>/versions/node/<version>/bin`: the version is the folder above `bin`
    if let Some(v) = var("NVM_BIN").and_then(|p| Path::new(p).parent().and_then(last)) {
        out.push(v);
    }
    out
}

/// A symbol made of `"<sgr>:<text>"` pieces, each painted with its own colour.
fn pieces(shell: Shell, list: &[String], status: i32) -> String {
    list.iter()
        .filter_map(|p| p.split_once(':'))
        .map(|(sgr, text)| paint(shell, sgr, &text.replace("{status}", &status.to_string())))
        .collect()
}

/// The prompt of a theme with a layout: `(left, right)`. Parts that have nothing to show are
/// skipped; the left parts are joined with a space and followed by one, the right parts are joined
/// without a space.
pub fn render_layout(
    shell: Shell,
    cfg: &PromptSection,
    plugins: &BTreeMap<String, PluginSettings>,
    pal: &Palette,
    layout: &Layout,
    input: &Input,
) -> (String, String) {
    // a theme may lower the threshold, unless the user chose their own
    let min_duration = match layout.min_duration_ms {
        Some(m) if cfg.min_duration_ms == PromptSection::default().min_duration_ms => m,
        _ => cfg.min_duration_ms,
    };
    let part = |name: &str| -> String {
        match name {
            "remote" => input
                .remote
                .as_deref()
                .map_or_else(String::new, |r| paint(shell, pal.dim, r)),
            "root" if input.root => paint(shell, &layout.root_sgr, &layout.root_text),
            "venv" => env_names(&input.env)
                .iter()
                .map(|n| paint(shell, pal.ok, &format!("{}{n}", layout.venv_prefix)))
                .collect::<Vec<_>>()
                .join(" "),
            "aws" => input
                .env
                .get("AWS_PROFILE")
                .or_else(|| input.env.get("AWS_DEFAULT_PROFILE"))
                .filter(|p| !p.is_empty())
                .map_or_else(String::new, |p| {
                    paint(shell, pal.duration, &format!("{}{p}", layout.aws_prefix))
                }),
            "dir" => {
                let mut d = paint(
                    shell,
                    pal.dir,
                    &format!(
                        "{}{}",
                        layout.dir_prefix,
                        short_dir(&input.cwd, input.home.as_deref())
                    ),
                );
                if input.readonly && !layout.dir_lock.is_empty() {
                    d.push_str(&paint(shell, pal.err, &layout.dir_lock));
                }
                d
            }
            "segments" => segments(shell, cfg, plugins, pal, input)
                .trim_start()
                .to_string(),
            "duration" if min_duration > 0 && input.duration_ms >= min_duration => paint(
                shell,
                pal.duration,
                &layout
                    .duration_format
                    .replace("{d}", &human_duration(input.duration_ms)),
            ),
            "status" if input.status != 0 => paint(shell, pal.err, &format!("[{}]", input.status)),
            "jobs" if input.jobs > 0 => paint(
                shell,
                pal.tool,
                &layout.jobs_format.replace("{n}", &input.jobs.to_string()),
            ),
            "symbol" => {
                let list = if input.status == 0 {
                    &layout.symbol_ok
                } else {
                    &layout.symbol_err
                };
                if list.is_empty() {
                    let color = if input.status == 0 { pal.ok } else { pal.err };
                    paint(shell, color, &cfg.symbol)
                } else {
                    pieces(shell, list, input.status)
                }
            }
            _ => String::new(),
        }
    };
    let join = |names: &[String], sep: &str| {
        names
            .iter()
            .map(|n| part(n))
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(sep)
    };
    let mut left = join(&layout.left, " ");
    left.push(' ');
    (left, join(&layout.right, ""))
}

/// Only the plugin segments (git, versions, ...), each preceded by a space: the part of the prompt
/// that needs this program. The zsh init script draws the rest itself.
pub fn segments(
    shell: Shell,
    cfg: &PromptSection,
    plugins: &BTreeMap<String, PluginSettings>,
    pal: &Palette,
    input: &Input,
) -> String {
    let mut out = String::new();
    let env = |k: &str| std::env::var(k).ok();
    let base = Ctx {
        cwd: &input.cwd,
        home: input.home.as_deref(),
        cache_dir: input.cache_dir.as_deref(),
        env: &env,
        opt: &|_| None,
    };
    let baked = input
        .cache_dir
        .as_deref()
        .map(shellcraft_plugin::store::load_baked)
        .unwrap_or_default();
    let draw = |segs: Vec<shellcraft_plugin::Segment>, out: &mut String| {
        for seg in segs {
            out.push(' ');
            for (text, tone) in seg.parts {
                out.push_str(&paint(shell, tone_code(pal, tone), &text));
            }
        }
    };
    for name in &cfg.segments {
        let settings = plugins.get(name).cloned().unwrap_or_default();
        if !settings.enabled {
            continue;
        }
        let opt = |k: &str| settings.options.get(k).cloned();
        let ctx = Ctx { opt: &opt, ..base };
        if let Some(p) = shellcraft_bundled::find(name) {
            draw(p.segments(&ctx), &mut out);
        } else if settings.segment.is_some() {
            draw(
                shellcraft_plugin::external::segments(name, settings.ttl_ms, &ctx),
                &mut out,
            );
        } else {
            // installed plugin: `name` is all its segments, `name.id` is one of them
            for row in baked
                .iter()
                .filter(|r| r.plugin == *name || format!("{}.{}", r.plugin, r.id) == *name)
            {
                draw(
                    shellcraft_plugin::external::baked_segments(row, &ctx),
                    &mut out,
                );
            }
        }
    }
    // installed plugins that are not listed yet still show, after the listed ones
    for row in &baked {
        let listed = cfg
            .segments
            .iter()
            .any(|s| *s == row.plugin || *s == format!("{}.{}", row.plugin, row.id));
        if !listed && plugins.get(&row.plugin).map_or(true, |p| p.enabled) {
            let opt = |_: &str| None;
            let ctx = Ctx { opt: &opt, ..base };
            draw(
                shellcraft_plugin::external::baked_segments(row, &ctx),
                &mut out,
            );
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(cwd: &str) -> Input {
        Input {
            cwd: PathBuf::from(cwd),
            home: Some(PathBuf::from("/home/u")),
            ..Input::default()
        }
    }

    #[test]
    fn dir_abbreviation() {
        let h = Some(Path::new("/home/u"));
        assert_eq!(short_dir(Path::new("/home/u"), h), "~");
        assert_eq!(short_dir(Path::new("/home/u/a/b"), h), "~/a/b");
        assert_eq!(short_dir(Path::new("/home/u/a/b/c/d"), h), "\u{2026}/b/c/d");
        assert_eq!(short_dir(Path::new("/"), h), "/");
        assert_eq!(short_dir(Path::new("/usr/lib"), h), "/usr/lib");
    }

    #[test]
    fn durations() {
        assert_eq!(human_duration(450), "450ms");
        assert_eq!(human_duration(5_000), "5s");
        assert_eq!(human_duration(125_000), "2m5s");
        assert_eq!(human_duration(3_900_000), "1h5m");
    }

    #[test]
    fn escapes_per_shell() {
        assert_eq!(literal(Shell::Zsh, "50%"), "50%%");
        assert_eq!(literal(Shell::Bash, "a$b`c\\"), "a\\$b\\`c\\\\");
        assert_eq!(literal(Shell::Fish, "a$b%"), "a$b%");
    }

    #[test]
    fn non_printing_wrappers() {
        let cfg = PromptSection::default();
        let z = render(
            Shell::Zsh,
            &cfg,
            &BTreeMap::new(),
            &Palette::DEFAULT,
            None,
            &input("/tmp"),
        );
        assert!(z.contains("%{\u{1b}[34m%}/tmp"), "{z:?}");
        let b = render(
            Shell::Bash,
            &cfg,
            &BTreeMap::new(),
            &Palette::DEFAULT,
            None,
            &input("/tmp"),
        );
        assert!(b.contains("\\[\u{1b}[34m\\]/tmp"), "{b:?}");
        let f = render(
            Shell::Fish,
            &cfg,
            &BTreeMap::new(),
            &Palette::DEFAULT,
            None,
            &input("/tmp"),
        );
        assert!(!f.contains('%') && !f.contains("\\["), "{f:?}");
    }

    #[test]
    fn status_and_duration_segments() {
        let cfg = PromptSection::default();
        let mut i = input("/tmp");
        let ok = render(
            Shell::Fish,
            &cfg,
            &BTreeMap::new(),
            &Palette::DEFAULT,
            None,
            &i,
        );
        assert!(!ok.contains('[') || ok.contains("\u{1b}["));
        assert!(ok.contains("32m"), "green symbol");
        i.status = 2;
        i.duration_ms = 7_000;
        let bad = render(
            Shell::Fish,
            &cfg,
            &BTreeMap::new(),
            &Palette::DEFAULT,
            None,
            &i,
        );
        assert!(
            bad.contains("[2]") && bad.contains("7s") && bad.contains("31m"),
            "{bad:?}"
        );
        i.duration_ms = 1_999;
        assert!(!render(
            Shell::Fish,
            &cfg,
            &BTreeMap::new(),
            &Palette::DEFAULT,
            None,
            &i
        )
        .contains("1s"));
    }

    fn passionp() -> Layout {
        Layout {
            left: ["root", "venv", "aws", "dir", "segments", "symbol"]
                .map(String::from)
                .to_vec(),
            right: ["jobs", "duration"].map(String::from).to_vec(),
            dir_prefix: "D ".into(),
            dir_lock: " L".into(),
            symbol_ok: ["31:>", "33:>", "32:>"].map(String::from).to_vec(),
            symbol_err: ["31:{status} >>>".to_string()].to_vec(),
            duration_format: "[T {d}]".into(),
            jobs_format: "[J {n}]".into(),
            ..Layout::default()
        }
    }

    fn plain(s: &str) -> String {
        // drop the zero-width colour wrappers and escapes: what the user reads
        let mut out = String::new();
        let mut rest = s;
        while let Some(i) = rest.find("%{") {
            out.push_str(&rest[..i]);
            rest = &rest[rest[i..].find("%}").map_or(rest.len(), |j| i + j + 2)..];
        }
        out.push_str(rest);
        out
    }

    #[test]
    fn layout_draws_parts_symbol_pieces_and_the_right_side() {
        let cfg = PromptSection {
            min_duration_ms: 10,
            ..PromptSection::default()
        };
        let mut i = input("/home/u/code");
        i.duration_ms = 1234;
        i.jobs = 2;
        let (left, right) = render_layout(
            Shell::Zsh,
            &cfg,
            &BTreeMap::new(),
            &Palette::DEFAULT,
            &passionp(),
            &i,
        );
        assert_eq!(plain(&left), "D ~/code >>> ", "{left:?}");
        // each arrow has its own colour
        assert!(
            left.contains("%{\u{1b}[31m%}>%{\u{1b}[0m%}%{\u{1b}[33m%}>")
                && left.contains("[32m%}>"),
            "{left:?}"
        );
        assert_eq!(plain(&right), "[J 2][T 1s]", "{right:?}");
        // failure: the status is part of the symbol, read-only and root show their marks
        i.status = 130;
        i.readonly = true;
        i.root = true;
        let (left, _) = render_layout(
            Shell::Zsh,
            &cfg,
            &BTreeMap::new(),
            &Palette::DEFAULT,
            &passionp(),
            &i,
        );
        assert_eq!(plain(&left), " ROOT  D ~/code L 130 >>> ", "{left:?}");
        // nothing to show on the right: empty
        i.jobs = 0;
        i.duration_ms = 0;
        let (_, right) = render_layout(
            Shell::Zsh,
            &cfg,
            &BTreeMap::new(),
            &Palette::DEFAULT,
            &passionp(),
            &i,
        );
        assert!(right.is_empty());
    }

    #[test]
    fn layout_default_symbol_and_environment_parts() {
        let layout = Layout {
            left: ["venv", "aws", "dir", "symbol"].map(String::from).to_vec(),
            venv_prefix: "py ".into(),
            aws_prefix: "aws ".into(),
            ..Layout::default()
        };
        let mut i = input("/tmp");
        i.env
            .insert("VIRTUAL_ENV".into(), "/work/proj/.venv-test".into());
        i.env.insert("AWS_PROFILE".into(), "dev".into());
        i.env.insert(
            "NVM_BIN".into(),
            "/home/u/.nvm/versions/node/v18.1.0/bin".into(),
        );
        let (left, _) = render_layout(
            Shell::Fish,
            &PromptSection::default(),
            &BTreeMap::new(),
            &Palette::DEFAULT,
            &layout,
            &i,
        );
        assert!(
            left.contains("py .venv-test")
                && left.contains("py v18.1.0")
                && left.contains("aws dev")
                && left.contains("/tmp"),
            "{left:?}"
        );
        assert!(
            left.ends_with("\u{1b}[0m "),
            "the configured symbol in the ok colour: {left:?}"
        );
    }
}
