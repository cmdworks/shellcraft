//! `shellcraft-plugin.toml`: what a plugin offers and what it asks for.

use std::collections::BTreeMap;

use serde::Deserialize;
use shellcraft_core::config::is_word;

pub const FILE_NAME: &str = "shellcraft-plugin.toml";
/// The plugin API version this shellcraft implements. A manifest declares the version it was
/// written for (`api = 1`); a newer one is refused with a clear message. Adding optional fields
/// does not change the number; a breaking change does.
pub const API_VERSION: u32 = 1;
const SHELLS: &[&str] = &["zsh", "bash", "fish", "nu", "pwsh"];

/// What a plugin is for. Informational for now (`feature` is the default); `profile` plugins carry
/// only `requires`.
pub const KINDS: &[&str] = &["feature", "segment", "shell", "theme", "tool", "profile"];

/// Code a plugin attaches to a named slot of one shell's init script.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SlotDef {
    pub shell: String,
    /// Slot name, as listed by the shell (see `shellcraft plugin slots <shell>`).
    pub name: String,
    /// File inside the plugin whose text is inserted unchanged.
    pub file: String,
    /// `before` or `after` the slot's own text (default `after`).
    #[serde(default = "default_position")]
    pub position: String,
}

fn default_position() -> String {
    "after".to_string()
}

/// A prebuilt executable the plugin needs, one entry per platform.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BinaryDef {
    /// Name used as `@name` in `command`, `run` and segment `command` fields.
    pub name: String,
    /// Rust target triple the build is for, e.g. `aarch64-apple-darwin`.
    pub target: String,
    /// `https://` URL (or, while developing, a file under `SHELLCRAFT_DEV_ARTIFACTS`).
    pub url: String,
    /// SHA-256 of the file at `url`, 64 hex digits.
    pub sha256: String,
    /// `raw` (default) or `tar.gz`.
    #[serde(default = "default_archive")]
    pub archive: String,
    /// For `tar.gz`: the file inside the archive (default: `name`).
    pub member: Option<String>,
}

fn default_archive() -> String {
    "raw".to_string()
}

/// A dependency such as `history` or `history>=1.2`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Requirement {
    pub name: String,
    pub min: Option<String>,
}

fn version_parts(v: &str) -> Vec<u64> {
    v.trim_start_matches('v')
        .split(['.', '-', '+'])
        .map_while(|p| p.parse().ok())
        .collect()
}

impl Requirement {
    pub fn parse(text: &str) -> Result<Self, String> {
        let (name, min) = match text.split_once(">=") {
            Some((n, v)) => (n.trim(), Some(v.trim())),
            None => (text.trim(), None),
        };
        if !is_word(name) {
            return Err(format!(
                "requirement `{text}`: `{name}` is not a plugin name"
            ));
        }
        if min.is_some_and(|v| version_parts(v).is_empty()) {
            return Err(format!(
                "requirement `{text}`: the version must start with a number"
            ));
        }
        Ok(Self {
            name: name.to_string(),
            min: min.map(str::to_string),
        })
    }

    /// Is `version` at least the minimum (no minimum: always)?
    pub fn satisfied_by(&self, version: &str) -> bool {
        let Some(min) = &self.min else { return true };
        let (have, want) = (version_parts(version), version_parts(min));
        let n = have.len().max(want.len());
        let pad = |v: &[u64]| {
            (0..n)
                .map(|i| v.get(i).copied().unwrap_or(0))
                .collect::<Vec<_>>()
        };
        pad(&have) >= pad(&want)
    }
}

/// Names of the bundled plugins; an installed plugin may not take them. `shellcraft-bundled` has a
/// test that keeps this list equal to the plugins it ships.
pub const RESERVED: &[&str] = &["git", "versions"];

/// Gate for a segment: show it only when one of these files exists in the project.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct When {
    pub files: Vec<String>,
}

/// A cheat sheet: a text file inside the plugin directory.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheatDef {
    pub file: String,
}

/// One prompt segment: an executable inside the plugin directory.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SegmentDef {
    /// Path relative to the plugin directory.
    pub command: String,
    #[serde(default = "default_ttl")]
    pub ttl_ms: u64,
    #[serde(default)]
    pub when: When,
}

/// A command that becomes `shellcraft <name>`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandDef {
    /// Executable inside the plugin directory; receives the arguments after the name.
    pub run: String,
    #[serde(default)]
    pub about: String,
}

/// Events a hook can react to.
pub const EVENTS: &[&str] = &["cd", "post"];

/// A program started (detached) when something happens in the shell.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HookDef {
    /// `cd` (directory changed) or `post` (a command finished).
    pub event: String,
    pub run: String,
}

pub fn default_kind() -> String {
    "feature".to_string()
}

fn default_api() -> u32 {
    1
}

fn default_ttl() -> u64 {
    10_000
}

/// What the plugin says it will do. Shown at install; not enforced by a sandbox.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct Permissions {
    /// Programs it runs.
    pub exec: Vec<String>,
    pub network: bool,
    /// Paths it writes.
    pub write: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// What the plugin is (default `feature`): one of [`KINDS`].
    #[serde(default = "default_kind")]
    pub kind: String,
    /// Plugins this one needs, e.g. `["history>=1"]`. Checked when it is installed.
    #[serde(default)]
    pub requires: Vec<String>,
    /// Plugins this one cannot be installed next to.
    #[serde(default)]
    pub conflicts: Vec<String>,
    /// Plugin API version this plugin was written for (default 1).
    #[serde(default = "default_api")]
    pub api: u32,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    pub license: Option<String>,
    /// Shells it supports (default: all).
    #[serde(default)]
    pub shells: Vec<String>,
    /// Segments by id.
    #[serde(default)]
    pub segment: BTreeMap<String, SegmentDef>,
    /// Commands by name: `shellcraft <name> ...`.
    #[serde(default)]
    pub command: BTreeMap<String, CommandDef>,
    /// Hooks by id. They receive SHELLCRAFT_EVENT, SHELLCRAFT_CWD and, for `post`,
    /// SHELLCRAFT_EXIT, SHELLCRAFT_DURATION_MS and SHELLCRAFT_COMMAND.
    #[serde(default)]
    pub hook: BTreeMap<String, HookDef>,
    /// Code attached to named slots of a shell's init script.
    #[serde(default)]
    pub slot: Vec<SlotDef>,
    /// Colour themes by name (`[theme.nord]`); any field left out is the default theme's.
    #[serde(default)]
    pub theme: BTreeMap<String, shellcraft_core::theme::ThemeColors>,
    /// Cheat sheets by topic (`[cheat.git] file = "cheats/git.txt"`), shown by `shellcraft cheat`.
    #[serde(default)]
    pub cheat: BTreeMap<String, CheatDef>,
    /// Prebuilt executables, one entry per platform; referenced as `@name`.
    #[serde(default)]
    pub binary: Vec<BinaryDef>,
    /// Shell name -> snippet file (relative path) added to that shell's init script.
    #[serde(default)]
    pub snippets: BTreeMap<String, String>,
    /// Data added to the config (your own entries win).
    #[serde(default)]
    pub aliases: BTreeMap<String, String>,
    #[serde(default)]
    pub abbr: BTreeMap<String, String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub permissions: Permissions,
}

/// A path that stays inside the plugin directory.
/// A path inside the plugin, or `@name` for one of its prebuilt binaries.
fn runnable(p: &str, binaries: &[BinaryDef]) -> bool {
    match p.strip_prefix('@') {
        Some(name) => binaries.iter().any(|b| b.name == name),
        None => safe_relative(p),
    }
}

fn safe_relative(p: &str) -> bool {
    !p.is_empty()
        && !p.starts_with('/')
        && !p.starts_with('~')
        && p.split('/').all(|c| !c.is_empty() && c != "..")
}

impl Manifest {
    /// Parse and check a manifest.
    pub fn parse(text: &str) -> Result<Self, String> {
        let m: Manifest = toml::from_str(text).map_err(|e| e.to_string())?;
        m.validate()?;
        Ok(m)
    }

    fn validate(&self) -> Result<(), String> {
        if self.api == 0 || self.api > API_VERSION {
            return Err(format!(
                "needs plugin API {}, this shellcraft supports up to {API_VERSION}; update shellcraft",
                self.api
            ));
        }
        if !is_word(&self.name) || self.name.contains('.') {
            return Err(format!(
                "name `{}`: use letters, digits, `-` or `_`",
                self.name
            ));
        }
        if RESERVED.contains(&self.name.as_str()) {
            return Err(format!("name `{}` is taken by a bundled plugin", self.name));
        }
        if self.version.trim().is_empty() {
            return Err("version is empty".into());
        }
        for s in &self.shells {
            if !SHELLS.contains(&s.as_str()) {
                return Err(format!(
                    "shells: unknown shell `{s}` (known: {})",
                    SHELLS.join(", ")
                ));
            }
        }
        for (id, seg) in &self.segment {
            if !is_word(id) || id.contains('.') {
                return Err(format!(
                    "segment id `{id}`: use letters, digits, `-` or `_`"
                ));
            }
            if !runnable(&seg.command, &self.binary) {
                return Err(format!(
                    "segment.{id}.command must be a relative path inside the plugin"
                ));
            }
            if seg.when.files.iter().any(|f| !safe_relative(f)) {
                return Err(format!(
                    "segment.{id}.when.files must be plain relative names"
                ));
            }
        }
        if !KINDS.contains(&self.kind.as_str()) {
            return Err(format!(
                "kind `{}`: known kinds are {}",
                self.kind,
                KINDS.join(", ")
            ));
        }
        for r in &self.requires {
            Requirement::parse(r)?;
        }
        for c in &self.conflicts {
            if !is_word(c) {
                return Err(format!("conflicts: `{c}` is not a plugin name"));
            }
        }
        for (i, sl) in self.slot.iter().enumerate() {
            if !SHELLS.contains(&sl.shell.as_str()) {
                return Err(format!("slot[{i}].shell: unknown shell `{}`", sl.shell));
            }
            if !is_word(&sl.name) {
                return Err(format!("slot[{i}].name `{}` is not a slot name", sl.name));
            }
            if !safe_relative(&sl.file) {
                return Err(format!(
                    "slot[{i}].file must be a relative path inside the plugin"
                ));
            }
            if !["before", "after"].contains(&sl.position.as_str()) {
                return Err(format!("slot[{i}].position must be `before` or `after`"));
            }
        }
        for (i, b) in self.binary.iter().enumerate() {
            if !is_word(&b.name) || b.name.contains('.') {
                return Err(format!(
                    "binary[{i}].name `{}`: use letters, digits, `-` or `_`",
                    b.name
                ));
            }
            if b.target.is_empty()
                || !b
                    .target
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "-_".contains(c))
            {
                return Err(format!(
                    "binary[{i}].target `{}` is not a target triple",
                    b.target
                ));
            }
            if b.sha256.len() != 64 || !b.sha256.bytes().all(|c| c.is_ascii_hexdigit()) {
                return Err(format!("binary[{i}].sha256 must be 64 hex digits"));
            }
            if b.url.trim().is_empty() {
                return Err(format!("binary[{i}].url is empty"));
            }
            if !["raw", "tar.gz"].contains(&b.archive.as_str()) {
                return Err(format!("binary[{i}].archive must be `raw` or `tar.gz`"));
            }
            if b.member.as_deref().is_some_and(|m| !safe_relative(m)) {
                return Err(format!(
                    "binary[{i}].member must be a relative path inside the archive"
                ));
            }
            if self
                .binary
                .iter()
                .take(i)
                .any(|o| o.name == b.name && o.target == b.target)
            {
                return Err(format!(
                    "binary `{}` is listed twice for {}",
                    b.name, b.target
                ));
            }
        }
        for (name, c) in &self.command {
            if !is_word(name) || name.contains('.') {
                return Err(format!("command `{name}`: use letters, digits, `-` or `_`"));
            }
            if !runnable(&c.run, &self.binary) {
                return Err(format!(
                    "command.{name}.run must be a relative path inside the plugin"
                ));
            }
        }
        for (id, h) in &self.hook {
            if !is_word(id) || id.contains('.') {
                return Err(format!("hook id `{id}`: use letters, digits, `-` or `_`"));
            }
            if !EVENTS.contains(&h.event.as_str()) {
                return Err(format!(
                    "hook.{id}.event `{}`: known events are {}",
                    h.event,
                    EVENTS.join(", ")
                ));
            }
            if !runnable(&h.run, &self.binary) {
                return Err(format!(
                    "hook.{id}.run must be a relative path inside the plugin"
                ));
            }
        }
        for (name, t) in &self.theme {
            if !is_word(name) || name.contains('.') || name == "default" {
                return Err(format!(
                    "theme `{name}`: use letters, digits, `-` or `_`; `default` is taken"
                ));
            }
            if let Some(why) = t.problem() {
                return Err(format!("theme.{name}: {why}"));
            }
        }
        for (topic, c) in &self.cheat {
            if !is_word(topic) || topic.contains('.') {
                return Err(format!("cheat `{topic}`: use letters, digits, `-` or `_`"));
            }
            if !safe_relative(&c.file) {
                return Err(format!(
                    "cheat.{topic}.file must be a relative path inside the plugin"
                ));
            }
        }
        for (shell, file) in &self.snippets {
            if !SHELLS.contains(&shell.as_str()) {
                return Err(format!("snippets: unknown shell `{shell}`"));
            }
            if !safe_relative(file) {
                return Err(format!(
                    "snippets.{shell} must be a relative path inside the plugin"
                ));
            }
        }
        for name in self.aliases.keys().chain(self.abbr.keys()) {
            if name.is_empty()
                || name.contains(|c: char| c.is_whitespace() || "=/$'\"\\".contains(c))
            {
                return Err(format!(
                    "alias or abbreviation name `{name}` is not allowed"
                ));
            }
        }
        for name in self.env.keys() {
            if name.is_empty()
                || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                || name.starts_with(|c: char| c.is_ascii_digit())
            {
                return Err(format!(
                    "env.{name}: variable names match [A-Za-z_][A-Za-z0-9_]*"
                ));
            }
        }
        Ok(())
    }

    /// The prebuilt binaries to install on `target` (one per name), and the names that have no
    /// build for it.
    pub fn binaries_for<'a>(&'a self, target: &str) -> (Vec<&'a BinaryDef>, Vec<&'a str>) {
        let mut names: Vec<&str> = self.binary.iter().map(|b| b.name.as_str()).collect();
        names.dedup();
        let mut have = Vec::new();
        let mut missing = Vec::new();
        for n in names {
            match self
                .binary
                .iter()
                .find(|b| b.name == n && b.target == target)
            {
                Some(b) => have.push(b),
                None => missing.push(n),
            }
        }
        (have, missing)
    }

    /// Does the plugin support `shell`?
    pub fn supports(&self, shell: &str) -> bool {
        self.shells.is_empty() || self.shells.iter().any(|s| s == shell)
    }

    /// Human-readable list of what it asks for, one item per line.
    pub fn permission_lines(&self) -> Vec<String> {
        let p = &self.permissions;
        let mut v = Vec::new();
        v.push(if p.exec.is_empty() {
            "runs: nothing declared".to_string()
        } else {
            format!("runs: {}", p.exec.join(", "))
        });
        v.push(format!("network: {}", if p.network { "yes" } else { "no" }));
        v.push(if p.write.is_empty() {
            "writes: nothing declared".to_string()
        } else {
            format!("writes: {}", p.write.join(", "))
        });
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The JSON schema documents every top-level manifest field (keep this list in step with
    /// `Manifest` and `docs/schema/plugin-manifest.schema.json`).
    #[test]
    fn schema_lists_every_manifest_field() {
        let schema = include_str!("../../docs/schema/plugin-manifest.schema.json");
        for key in [
            "api",
            "name",
            "version",
            "description",
            "license",
            "shells",
            "kind",
            "requires",
            "conflicts",
            "segment",
            "command",
            "hook",
            "slot",
            "snippets",
            "aliases",
            "abbr",
            "env",
            "theme",
            "cheat",
            "binary",
            "permissions",
        ] {
            assert!(
                schema.contains(&format!("\"{key}\": ")),
                "schema is missing `{key}`"
            );
        }
        // and the official plugins' manifests use only documented keys (unknown keys are errors)
        for k in KINDS {
            assert!(
                schema.contains(&format!("\"{k}\"")),
                "schema lacks kind {k}"
            );
        }
    }

    const GOOD: &str = r#"
name = "todos"
version = "0.1.0"
description = "count TODOs"
license = "MIT"
shells = ["zsh", "fish"]
requires = ["history>=1.2"]
kind = "feature"

[segment.count]
command = "bin/todos"
ttl_ms = 5000
when = { files = [".git"] }

[[slot]]
shell = "zsh"
name = "aliases"
file = "slots/aliases.zsh"
position = "before"

[command.todo]
run = "bin/todo"
about = "list TODOs"

[hook.note]
event = "cd"
run = "bin/on-cd"

[snippets]
zsh = "snippets/todos.zsh"

[aliases]
td = "todo list"

[permissions]
exec = ["grep"]
"#;

    #[test]
    fn parses_a_full_manifest() {
        let m = Manifest::parse(GOOD).expect("valid");
        assert_eq!(m.api, 1, "api defaults to 1");
        assert_eq!(m.kind, "feature");
        assert_eq!(m.requires, vec!["history>=1.2"]);
        assert_eq!(m.slot[0].name, "aliases");
        assert_eq!(m.slot[0].position, "before");
        assert_eq!(m.name, "todos");
        assert_eq!(m.segment["count"].ttl_ms, 5000);
        assert_eq!(m.segment["count"].when.files, vec![".git"]);
        assert!(m.supports("zsh") && !m.supports("bash"));
        assert_eq!(m.permission_lines()[0], "runs: grep");
        assert_eq!(m.permission_lines()[1], "network: no");
        assert_eq!(m.command["todo"].run, "bin/todo");
        assert_eq!(m.hook["note"].event, "cd");
    }

    #[test]
    fn binaries_parse_validate_and_select_by_target() {
        let hash = "a".repeat(64);
        let text = format!(
            "name = \"b\"\nversion = \"1\"\n[[binary]]\nname = \"tool\"\ntarget = \"x-linux\"\nurl = \"https://e/x\"\nsha256 = \"{hash}\"\n[[binary]]\nname = \"tool\"\ntarget = \"y-mac\"\nurl = \"t.tar.gz\"\nsha256 = \"{hash}\"\narchive = \"tar.gz\"\nmember = \"bin/tool\"\n[command.run]\nrun = \"@tool\"\n"
        );
        let m = Manifest::parse(&text).expect("valid");
        let (have, missing) = m.binaries_for("y-mac");
        assert_eq!((have.len(), missing.len()), (1, 0));
        assert_eq!(have[0].member.as_deref(), Some("bin/tool"));
        let (have, missing) = m.binaries_for("z-other");
        assert!(have.is_empty() && missing == vec!["tool"]);
        let bad = |extra: &str| {
            Manifest::parse(&format!("name = \"b\"\nversion = \"1\"\n{extra}")).expect_err(extra)
        };
        assert!(bad("[command.c]\nrun = \"@nope\"").contains("relative path"));
        let b = |f: &str| {
            format!(
                "[[binary]]\nname = \"t\"\ntarget = \"x\"\nurl = \"u\"\nsha256 = \"{}\"\n{f}",
                "a".repeat(64)
            )
        };
        assert!(bad(&b("archive = \"zip\"")).contains("raw` or `tar.gz"));
        assert!(
            bad("[[binary]]\nname = \"t\"\ntarget = \"x\"\nurl = \"u\"\nsha256 = \"short\"")
                .contains("64 hex")
        );
        assert!(bad(&b("member = \"../x\"")).contains("member"));
        assert!(bad(&format!("{}{}", b(""), b(""))).contains("listed twice"));
    }

    #[test]
    fn requirements_parse_and_compare_versions() {
        let r = Requirement::parse("history>=1.2").expect("valid");
        assert_eq!(
            (r.name.as_str(), r.min.as_deref()),
            ("history", Some("1.2"))
        );
        assert!(r.satisfied_by("1.2") && r.satisfied_by("1.10.0") && r.satisfied_by("v2"));
        assert!(!r.satisfied_by("1.1.9") && !r.satisfied_by("0.9"));
        assert!(Requirement::parse("plain")
            .expect("valid")
            .satisfied_by("0.0.1"));
        assert!(Requirement::parse("bad name").is_err());
        assert!(Requirement::parse(">=1").is_err());
    }

    #[test]
    fn rejects_unsafe_or_unknown_input() {
        let bad = |extra: &str| {
            Manifest::parse(&format!("name = \"x\"\nversion = \"1\"\n{extra}")).expect_err(extra)
        };
        assert!(bad("[segment.a]\ncommand = \"/bin/sh\"").contains("relative"));
        assert!(bad("[segment.a]\ncommand = \"../evil\"").contains("relative"));
        assert!(bad("[snippets]\nzsh = \"~/x\"").contains("relative"));
        assert!(bad("[snippets]\ncsh = \"x\"").contains("unknown shell"));
        assert!(bad("shells = [\"tcsh\"]").contains("unknown shell"));
        assert!(bad("[command.a]\nrun = \"/bin/ls\"").contains("relative"));
        assert!(bad("[command.\"a b\"]\nrun = \"x\"").contains("command"));
        assert!(bad("[hook.h]\nevent = \"startup\"\nrun = \"x\"").contains("known events"));
        assert!(bad("[hook.h]\nevent = \"cd\"\nrun = \"../x\"").contains("relative"));
        assert!(bad("kind = \"nonsense\"").contains("known kinds"));
        assert!(bad("requires = [\"a>=x\"]").contains("version must start with a number"));
        assert!(
            bad("[[slot]]\nshell = \"csh\"\nname = \"a\"\nfile = \"x\"").contains("unknown shell")
        );
        assert!(
            bad("[[slot]]\nshell = \"zsh\"\nname = \"a\"\nfile = \"../x\"").contains("relative")
        );
        assert!(bad(
            "[[slot]]\nshell = \"zsh\"\nname = \"a\"\nfile = \"x\"\nposition = \"middle\""
        )
        .contains("before` or `after"));
        assert!(bad("api = 2").contains("plugin API 2"));
        assert!(bad("api = 0").contains("plugin API 0"));
        assert!(bad("surprise = 1").contains("unknown field"));
        assert!(bad("[env]\n\"1X\" = \"v\"").contains("env."));
        assert!(Manifest::parse("name = \"git\"\nversion = \"1\"")
            .expect_err("taken")
            .contains("bundled"));
        assert!(Manifest::parse("name = \"a b\"\nversion = \"1\"").is_err());
        assert!(Manifest::parse("name = \"a\"\nversion = \" \"").is_err());
    }
}
