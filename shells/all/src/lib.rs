//! All shells behind one lookup.
//!
//! Re-exports the shell API so callers need one dependency: `shellcraft_shell::{emitter,
//! ShellKind, ShellEmitter, quote_for, rc, ...}`.

mod assemble;
mod select;

pub use assemble::{attach_bundled, available_themes, generate, load_plugins, path_line};
pub use select::{env, pick_shell, rc_path};
pub use shellcraft_shell_api::{
    config_home, keys, quote_for, rc, var, EmitError, ShellEmitter, ShellKind,
};
pub use shellcraft_shell_bash::Bash;
pub use shellcraft_shell_fish::Fish;
pub use shellcraft_shell_nushell::Nushell;
pub use shellcraft_shell_pwsh::Pwsh;
pub use shellcraft_shell_zsh::Zsh;

/// Emitter for `kind`.
pub fn emitter(kind: ShellKind) -> &'static dyn ShellEmitter {
    match kind {
        ShellKind::Zsh => &Zsh,
        ShellKind::Bash => &Bash,
        ShellKind::Fish => &Fish,
        ShellKind::Nushell => &Nushell,
        ShellKind::Pwsh => &Pwsh,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shellcraft_core::Config;

    const ALL: [ShellKind; 5] = [
        ShellKind::Zsh,
        ShellKind::Bash,
        ShellKind::Fish,
        ShellKind::Nushell,
        ShellKind::Pwsh,
    ];

    #[test]
    fn every_shell_renders_header() {
        for kind in ALL {
            let out = emitter(kind).emit(&Config::default()).unwrap();
            assert!(
                out.starts_with("# shellcraft: generated for"),
                "{kind:?}: {out}"
            );
            assert!(out.contains(kind.name()));
        }
    }

    #[test]
    fn quoting_in_every_shell_goes_through_the_same_function() {
        for kind in ALL {
            assert_eq!(emitter(kind).quote("a b"), quote_for(kind, "a b"));
        }
    }
}
