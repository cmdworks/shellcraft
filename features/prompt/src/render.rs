//! Prompt renderer: `[user@host ]dir [branch] [took 3s] ❯ `.
//!
//! Pure string building; everything after the directory comes from plugin segments
//! (`shellcraft-plugin`), so a render stays far below the 5 ms budget.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use shellcraft_core::theme::Palette;
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
}

fn tone_code(pal: &Palette, tone: Tone) -> &'static str {
    match tone {
        Tone::Git => pal.git,
        Tone::Version => pal.version,
        Tone::Tool => pal.tool,
        Tone::Warn => pal.duration,
        Tone::Dim => pal.dim,
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

/// Render the prompt for `shell`.
pub fn render(
    shell: Shell,
    cfg: &PromptSection,
    plugins: &BTreeMap<String, PluginSettings>,
    pal: &Palette,
    input: &Input,
) -> String {
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
            &input("/tmp"),
        );
        assert!(z.contains("%{\u{1b}[34m%}/tmp"), "{z:?}");
        let b = render(
            Shell::Bash,
            &cfg,
            &BTreeMap::new(),
            &Palette::DEFAULT,
            &input("/tmp"),
        );
        assert!(b.contains("\\[\u{1b}[34m\\]/tmp"), "{b:?}");
        let f = render(
            Shell::Fish,
            &cfg,
            &BTreeMap::new(),
            &Palette::DEFAULT,
            &input("/tmp"),
        );
        assert!(!f.contains('%') && !f.contains("\\["), "{f:?}");
    }

    #[test]
    fn status_and_duration_segments() {
        let cfg = PromptSection::default();
        let mut i = input("/tmp");
        let ok = render(Shell::Fish, &cfg, &BTreeMap::new(), &Palette::DEFAULT, &i);
        assert!(!ok.contains('[') || ok.contains("\u{1b}["));
        assert!(ok.contains("32m"), "green symbol");
        i.status = 2;
        i.duration_ms = 7_000;
        let bad = render(Shell::Fish, &cfg, &BTreeMap::new(), &Palette::DEFAULT, &i);
        assert!(
            bad.contains("[2]") && bad.contains("7s") && bad.contains("31m"),
            "{bad:?}"
        );
        i.duration_ms = 1_999;
        assert!(!render(Shell::Fish, &cfg, &BTreeMap::new(), &Palette::DEFAULT, &i).contains("1s"));
    }
}
