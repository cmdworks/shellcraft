//! Plugin API. A plugin can add prompt segments, shell snippets, or both.
//!
//! The bundled plugins (git, versions) use the same interface that outside plugins get
//! through [`external`], so a segment is one thing whether it is built in or not.

use std::path::Path;

use shellcraft_core::Shell;

pub mod external;
pub mod manifest;
pub mod store;

/// Colour role of a text part; the prompt maps it onto the active theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Git,
    Version,
    Tool,
    Warn,
    Dim,
}

/// One prompt segment: text parts, each with its own tone (`main` + `*`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    pub parts: Vec<(String, Tone)>,
}

impl Segment {
    pub fn new(text: impl Into<String>, tone: Tone) -> Self {
        Self {
            parts: vec![(text.into(), tone)],
        }
    }

    pub fn push(mut self, text: impl Into<String>, tone: Tone) -> Self {
        self.parts.push((text.into(), tone));
        self
    }
}

/// What a plugin may look at when drawing a prompt.
#[derive(Clone, Copy)]
pub struct Ctx<'a> {
    pub cwd: &'a Path,
    pub home: Option<&'a Path>,
    /// shellcraft cache directory (plugins keep small caches here).
    pub cache_dir: Option<&'a Path>,
    pub env: &'a dyn Fn(&str) -> Option<String>,
    /// This plugin's `[plugin.<name>.options]` values.
    pub opt: &'a dyn Fn(&str) -> Option<String>,
}

/// A feature that plugs into the prompt and/or the shell init script.
pub trait Plugin: Sync {
    /// Name used in `[prompt] segments` and `[plugin.<name>]`.
    fn name(&self) -> &'static str;

    /// One-line description for `shellcraft plugin list`.
    fn describe(&self) -> &'static str;

    /// Prompt segments for the current directory (empty when not relevant).
    fn segments(&self, _ctx: &Ctx) -> Vec<Segment> {
        Vec::new()
    }

    /// Shell code added to the generated init script, if this plugin has any for `shell`.
    fn snippet(&self, _shell: Shell) -> Option<&'static str> {
        None
    }
}
