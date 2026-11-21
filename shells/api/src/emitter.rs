//! The `ShellEmitter` trait: one implementation per shell, rendering minijinja templates.

use std::path::PathBuf;

use minijinja::{value::Serde, Environment, Error as TemplateError, Value};
use shellcraft_core::{Config, SlotPosition};
use thiserror::Error;

use crate::context::build;
use crate::quote::quote_for;
use crate::ShellKind;

/// Failure while generating an init script.
#[derive(Debug, Error)]
pub enum EmitError {
    #[error("invalid key `{key}`: {reason}")]
    Key { key: String, reason: String },
    #[error("template error for {shell}: {source}")]
    Template {
        shell: &'static str,
        #[source]
        source: TemplateError,
    },
}

/// Non-empty value of an environment variable.
pub fn var(env: &dyn Fn(&str) -> Option<String>, key: &str) -> Option<String> {
    env(key).filter(|v| !v.is_empty())
}

/// `$XDG_CONFIG_HOME`, else `$HOME/.config`.
pub fn config_home(env: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    match var(env, "XDG_CONFIG_HOME") {
        Some(x) => Some(PathBuf::from(x)),
        None => Some(PathBuf::from(var(env, "HOME")?).join(".config")),
    }
}

/// Generates the init script for one shell.
pub trait ShellEmitter {
    /// Shell this emitter targets.
    fn kind(&self) -> ShellKind;

    /// Templates of this shell as `(name, source)`, named `<shell>/<slot>`.
    fn templates(&self) -> &'static [(&'static str, &'static str)];

    /// The slots of this shell's init script, in order. Each is rendered from the template named
    /// `<shell>/<slot>`; plugins may attach code before or after any slot (`[[slot]]`).
    fn slots(&self) -> &'static [&'static str];

    /// Quote `s` as a single literal word for this shell.
    fn quote(&self, s: &str) -> String {
        quote_for(self.kind(), s)
    }

    /// The rc file where this shell reads the init script (`~/.zshrc`, `config.fish`, ...), from
    /// an environment lookup. `None` when no home directory is known.
    fn rc_file(&self, env: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf>;

    /// Command line (after the tool name) whose output initialises `tool` in this shell,
    /// or `None` if the shell has no support for it.
    fn tool_init(&self, _tool: &str) -> Option<&'static str> {
        None
    }

    /// Key sequence for a `[keys]` entry such as `ctrl-r`. `None` if unsupported here.
    fn key_sequence(&self, _key: &str) -> Option<String> {
        None
    }

    /// Render the init script for `config`.
    fn emit(&self, config: &Config) -> Result<String, EmitError> {
        // Theme settings become env vars unless the user set them in [env].
        let mut themed = config.clone();
        let pal = config.palette();
        for (k, v) in [("FZF_DEFAULT_OPTS", pal.fzf), ("BAT_THEME", pal.bat)] {
            if !v.is_empty() {
                themed
                    .env
                    .entry(k.to_string())
                    .or_insert_with(|| v.to_string());
            }
        }
        let ctx = build(self, &themed)?;
        let shell = self.kind().name();
        let wrap = |source| EmitError::Template { shell, source };
        let kind = self.kind();
        let mut env = Environment::new();
        env.add_filter("q", move |s: String| quote_for(kind, &s));
        for (name, source) in self.templates() {
            env.add_template(name, source).map_err(wrap)?;
        }
        let value = Value::from(Serde(ctx));
        let mut pieces: Vec<String> = Vec::new();
        let attached = |slot: &str, position: SlotPosition| -> Vec<String> {
            config
                .installed
                .iter()
                .flat_map(|p| p.slots.iter())
                .filter(|s| s.slot == slot && s.position == position)
                .map(|s| s.text.trim_end_matches('\n').to_string())
                .collect()
        };
        for slot in self.slots() {
            pieces.extend(attached(slot, SlotPosition::Before));
            let tmpl = env.get_template(&format!("{shell}/{slot}")).map_err(wrap)?;
            pieces.push(tmpl.render(value.clone()).map_err(wrap)?);
            pieces.extend(attached(slot, SlotPosition::After));
        }
        let mut out = pieces.join("\n");
        if !out.ends_with('\n') {
            out.push('\n');
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(ShellKind::Zsh.name(), "zsh");
        assert_eq!(ShellKind::Nushell.name(), "nu");
    }

    #[test]
    fn quote_filter_available_in_templates() {
        struct T;
        impl ShellEmitter for T {
            fn kind(&self) -> ShellKind {
                ShellKind::Bash
            }
            fn rc_file(&self, _env: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
                None
            }
            fn slots(&self) -> &'static [&'static str] {
                &["main"]
            }
            fn templates(&self) -> &'static [(&'static str, &'static str)] {
                &[(
                    "bash/main",
                    "{% for k, v in env|items %}{{ k }}={{ v|q }}\n{% endfor %}",
                )]
            }
        }
        let mut cfg = Config::default();
        cfg.env.insert("A".into(), "it's".into());
        let out = T.emit(&cfg).unwrap();
        assert_eq!(out, "A='it'\\''s'\n");
    }
}
