//! The values handed to every template.

use serde::Serialize;
use shellcraft_core::Config;

use crate::emitter::{EmitError, ShellEmitter};
use crate::ShellKind;

/// Template context: the config plus values derived from it.
#[derive(Serialize)]
pub(crate) struct Context<'a> {
    #[serde(flatten)]
    config: &'a Config,
    inits: Vec<ToolInit>,
    /// atuin is loaded and owns Ctrl+R, so shellcraft does not bind it.
    atuin: bool,
    unknown_tools: Vec<String>,
    /// Known tools that are enabled but have no init support in this shell.
    unsupported_tools: Vec<&'static str>,
    bindings: Vec<Binding>,
    /// Shell code contributed by enabled plugins.
    plugin_snippets: Vec<String>,
    /// Events (`cd`, `post`) some installed plugin has a hook for.
    hook_events: &'a [String],
    /// Theme colours for the shell's own syntax highlighting.
    hl: crate::colors::Highlight,
    /// The theme palette (SGR codes), for prompts a shell draws itself.
    pal: shellcraft_core::theme::Palette,
}

#[derive(Serialize)]
struct ToolInit {
    name: &'static str,
    command: String,
}

#[derive(Serialize)]
struct Binding {
    sequence: String,
    command: String,
}

/// Tools known to shellcraft, in load order (atuin last so it owns Ctrl+R).
const TOOLS: [(&str, &str); 5] = [
    ("starship", "starship"),
    ("zoxide", "zoxide"),
    ("fzf", "fzf"),
    ("mise", "mise"),
    ("atuin", "atuin"),
];

/// Everything the templates need besides the config itemitter.
pub(crate) fn build<'a, E: ShellEmitter + ?Sized>(
    emitter: &E,
    config: &'a Config,
) -> Result<Context<'a>, EmitError> {
    {
        let mut inits = Vec::new();
        let mut unsupported_tools = Vec::new();
        for (name, bin) in TOOLS {
            let has_builtin_prompt = matches!(
                emitter.kind(),
                ShellKind::Zsh | ShellKind::Bash | ShellKind::Fish
            );
            if name == "starship" && config.prompt.builtin && has_builtin_prompt {
                continue; // the built-in prompt replaces it
            }
            if config.tools.get(name).copied().unwrap_or(false) {
                match emitter.tool_init(name) {
                    Some(args) => inits.push(ToolInit {
                        name,
                        command: format!("{bin} {args}"),
                    }),
                    None => unsupported_tools.push(name),
                }
            }
        }
        let unknown_tools = config
            .tools
            .iter()
            .filter(|(n, on)| **on && !TOOLS.iter().any(|(k, _)| k == n))
            .map(|(n, _)| n.clone())
            .collect();
        let mut bindings = Vec::new();
        for (key, command) in &config.keys {
            let sequence = emitter.key_sequence(key).ok_or_else(|| EmitError::Key {
                key: key.clone(),
                reason: format!("not supported for {}", emitter.kind().name()),
            })?;
            bindings.push(Binding {
                sequence,
                command: command.clone(),
            });
        }
        // Shell code from plugins (bundled or installed; `apply` attaches both to the config).
        let plugin_snippets = config
            .installed
            .iter()
            .filter(|p| config.plugin.get(&p.name).map_or(true, |s| s.enabled))
            .filter_map(|p| p.snippets.get(emitter.kind().name()).cloned())
            .collect();
        Ok(Context {
            config,
            plugin_snippets,
            hook_events: &config.hook_events,
            hl: crate::colors::highlight(&config.palette()),
            pal: config.palette(),
            atuin: inits.iter().any(|t| t.name == "atuin"),
            inits,
            unknown_tools,
            unsupported_tools,
            bindings,
        })
    }
}
