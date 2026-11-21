//! shellcraft entry point: the clap tree and dispatch. Each command's arguments and code live in
//! its feature crate (`features/*`); the setup commands that orchestrate several crates are in
//! `setup/`.

mod setup;

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use setup::{apply, init, theme, uninstall, wizard};
use shellcraft_core::paths;
use shellcraft_plugins_cli::{Host, PluginCmd, RegistryCmd};

#[derive(Parser)]
#[command(
    name = "shellcraft",
    version,
    about = "Fast single-binary shell customization"
)]
pub struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Write a commented default config
    Init {
        /// Overwrite an existing config
        #[arg(long)]
        force: bool,
    },
    /// Remove the rc wiring and generated files (the config is kept)
    Uninstall {
        /// Also delete the whole cache directory (tool init caches, completion dump)
        #[arg(long)]
        purge: bool,
    },
    /// Check config, tools, rc wiring and generated script
    Doctor(shellcraft_health::doctor::DoctorArgs),
    /// Print shell completions for shellcraft itself
    Completions {
        /// bash, zsh, fish, elvish or powershell
        shell: clap_complete::Shell,
    },
    /// Startup time, disk use, history and tool versions
    Stats(shellcraft_health::measure::StatsArgs),
    /// Measure interactive shell startup (default 20 runs)
    Bench(shellcraft_health::measure::BenchArgs),
    /// Show where zsh startup time goes (zprof)
    Profile,
    /// Render the built-in prompt ([prompt] builtin = true wires it into your shell)
    Prompt(shellcraft_prompt::PromptArgs),
    /// List themes, or switch to one (prompt, fzf and bat colours) and re-apply
    Theme {
        name: Option<String>,
        #[arg(long)]
        shell: Option<String>,
    },
    /// Print the closest command names in PATH (command-not-found and `fix` use this)
    Suggest(shellcraft_not_found::SuggestArgs),
    /// Install plugins by name (`shellcraft install docker`), or tools with your package manager
    Install(InstallArgs),
    /// Update shellcraft and all installed plugins to their latest versions
    Update {
        /// Only look for a newer shellcraft; change nothing
        #[arg(long)]
        check: bool,
    },
    /// Remove an installed plugin
    Remove { name: String },
    /// List installed and bundled plugins
    List,
    /// Search the official plugins and your registries
    Search { query: Option<String> },
    /// Ask a few questions, write the config and apply it
    Setup,
    /// Full-screen manager: features, plugins, theme (also what plain `shellcraft` opens on a terminal)
    #[cfg(feature = "tui")]
    Tui,
    /// Short cheat sheets (no topic lists them)
    #[cfg(feature = "coreutils")]
    Cheat(shellcraft_coreutils::CheatArgs),
    /// Show files with line numbers and syntax colours on a terminal; plain `cat` when piped
    #[cfg(feature = "coreutils")]
    Cat(shellcraft_coreutils::CatArgs),
    /// List directory contents (eza replacement): colours, -l, -a, tree
    #[cfg(feature = "coreutils")]
    Ls(shellcraft_coreutils::LsArgs),
    /// List files (or directories) under a path, honouring .gitignore
    #[cfg(feature = "coreutils")]
    Files(shellcraft_coreutils::FilesArgs),
    /// Fuzzy picker: choose one line from stdin (replaces fzf)
    Pick(shellcraft_picker::PickArgs),
    /// List plugins (git, versions and your own)
    Plugin {
        #[command(subcommand)]
        cmd: PluginCmd,
    },
    /// Plugin registries: where `plugin add <name>` and `plugin search` look
    Registry {
        #[command(subcommand)]
        cmd: RegistryCmd,
    },
    /// Replace your config with a profile repository's and install its pinned plugins
    Pull(shellcraft_profile::PullArgs),
    /// Commit your config and lock file back to the profile repository
    Push(shellcraft_profile::PushArgs),
    /// Run the hooks of installed plugins for an event (the shell calls this)
    #[command(hide = true)]
    Hook(shellcraft_plugins_cli::HookArgs),
    /// A command from an installed plugin
    #[command(external_subcommand)]
    External(Vec<String>),
    /// Completion candidates for a command line, learned from `--help` (the shell calls this)
    Complete(shellcraft_complete::CompleteArgs),
    /// Command history (Ctrl-R uses this)
    Hist {
        #[command(subcommand)]
        cmd: shellcraft_history::HistCmd,
    },
    /// Frecency directory jumping (shell functions `j` and `ji` use this)
    Jump {
        #[command(subcommand)]
        cmd: shellcraft_jump::JumpCmd,
    },
    /// Print the config file path
    Path,
    /// Generate the init script for a shell
    Apply {
        /// zsh, bash or fish (default: [shell] default, then $SHELL)
        #[arg(long)]
        shell: Option<String>,
        /// Print the script to stdout instead of writing it
        #[arg(long, conflicts_with = "rc")]
        print: bool,
        /// Add a marked block to the shell rc file (a backup is made)
        #[arg(long)]
        rc: bool,
    },
}

fn config_path() -> Result<PathBuf> {
    paths::config_path(|k| std::env::var(k).ok())
        .context("cannot locate config: set SHELLCRAFT_CONFIG or HOME")
}

/// `shellcraft ls | head` closes our stdout early; Rust's `println!` panics then. Exit quietly with
/// the conventional status (128 + SIGPIPE) instead of printing a panic message.
fn quiet_broken_pipe() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if info.to_string().contains("Broken pipe") {
            std::process::exit(141);
        }
        default(info);
    }));
}

/// Names of the built-in commands (hidden ones included), for plugin conflict checks.
fn builtin_commands() -> Vec<String> {
    <Cli as clap::CommandFactory>::command()
        .get_subcommands()
        .map(|c| c.get_name().to_string())
        .collect()
}

/// Names of the visible built-in commands, for did-you-mean hints.
fn visible_commands() -> Vec<String> {
    <Cli as clap::CommandFactory>::command()
        .get_subcommands()
        .filter(|c| !c.is_hide_set())
        .map(|c| c.get_name().to_string())
        .collect()
}

/// Run `f` with the plugin host: `reapply` regenerates the init script quietly after a change.
fn with_host<T>(
    builtin: &[String],
    reapply: bool,
    f: impl FnOnce(&Host) -> Result<T>,
) -> Result<T> {
    let config_path = config_path()?;
    let reapply = |p: &std::path::Path| -> Result<()> {
        if !reapply {
            return Ok(());
        }
        apply::set_quiet(true);
        let result = apply::run(p, None, false, false);
        apply::set_quiet(false);
        result
    };
    f(&Host {
        config_path: &config_path,
        builtin_commands: builtin,
        reapply: &reapply,
    })
}

/// Arguments of `shellcraft install`.
#[derive(clap::Args)]
struct InstallArgs {
    /// Plugins (official name, `owner/repo[@rev]`, folder or git URL) or tools such as `fzf`;
    /// no names installs the tools enabled in the config
    names: Vec<String>,
    /// Tools: print the commands without running them
    #[arg(long)]
    dry_run: bool,
    /// Tools: download a checksummed release binary instead of using a package manager
    #[arg(long)]
    native: bool,
}

/// Look for a newer shellcraft (a failure to reach the release site is a note, not an error).
#[cfg(feature = "installer")]
fn update_self(check_only: bool) {
    use shellcraft_installer::selfupdate::{run, Outcome, DEFAULT_BASE};
    let custom = std::env::var("SHELLCRAFT_UPDATE_URL").ok();
    if custom.is_none() && shellcraft_core::proc::offline() {
        println!("offline: not checking for a newer shellcraft");
        return;
    }
    let base = custom.unwrap_or_else(|| DEFAULT_BASE.to_string());
    let current = env!("CARGO_PKG_VERSION");
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    match run(&base, current, &exe, check_only) {
        Ok(Outcome::UpToDate(v)) => println!("shellcraft {v} is the latest"),
        Ok(Outcome::Available(v)) => {
            println!("shellcraft {v} is available (you have {current}): run `shellcraft update`")
        }
        Ok(Outcome::Updated { from, to }) => {
            println!("shellcraft {from} -> {to}: restart your shell to use it")
        }
        Ok(Outcome::Managed(how)) => {
            println!("shellcraft was installed by a package manager: run `{how}`")
        }
        Err(e) => println!("could not check for a newer shellcraft: {e:#}"),
    }
}

#[cfg(not(feature = "installer"))]
fn update_self(_: bool) {
    println!("this build cannot update itself; reinstall to get a newer shellcraft");
}

#[cfg(feature = "installer")]
fn install_tools(
    tools: Vec<String>,
    dry_run: bool,
    native: bool,
    config: &std::path::Path,
) -> Result<()> {
    shellcraft_installer::run(
        shellcraft_installer::InstallArgs {
            tools,
            dry_run,
            native,
        },
        config,
    )
}

#[cfg(not(feature = "installer"))]
fn install_tools(tools: Vec<String>, _: bool, _: bool, _: &std::path::Path) -> Result<()> {
    anyhow::bail!(
        "{}: not a plugin (`shellcraft search`); this build has no tool installer",
        tools.join(", ")
    )
}

#[cfg(feature = "tui")]
fn run_tui() -> Result<()> {
    with_host(&builtin_commands(), true, |host| {
        setup::tui::run(host.config_path, host)
    })
}

fn main() -> Result<()> {
    quiet_broken_pipe();
    // no arguments on a terminal opens the manager; `--help` and every other form are unchanged
    #[cfg(feature = "tui")]
    if std::env::args_os().len() == 1 && std::io::IsTerminal::is_terminal(&std::io::stdout()) {
        return run_tui();
    }
    match Cli::parse().command {
        #[cfg(feature = "tui")]
        Command::Tui => run_tui(),
        Command::Init { force } => init::run(&config_path()?, force),
        Command::Apply { shell, print, rc } => {
            apply::run(&config_path()?, shell.as_deref(), print, rc)
        }
        Command::Uninstall { purge } => uninstall::run(purge),
        Command::Doctor(a) => shellcraft_health::doctor::run(&config_path()?, a),
        Command::Completions { shell } => {
            let mut cmd = <Cli as clap::CommandFactory>::command();
            clap_complete::generate(shell, &mut cmd, "shellcraft", &mut std::io::stdout());
            Ok(())
        }
        Command::Stats(a) => shellcraft_health::measure::stats(&config_path()?, a),
        Command::Bench(a) => shellcraft_health::measure::bench(&config_path()?, a),
        Command::Profile => shellcraft_health::measure::profile(),
        Command::Prompt(a) => shellcraft_prompt::run(&config_path()?, a),
        Command::Theme { name, shell } => theme::run(&config_path()?, name, shell.as_deref()),
        Command::Suggest(a) => shellcraft_not_found::run(a),
        Command::Install(a) => with_host(&builtin_commands(), true, |h| {
            let rest = if a.names.is_empty() {
                Vec::new()
            } else {
                shellcraft_plugins_cli::install_names(&a.names, h)?
            };
            if a.names.is_empty() || !rest.is_empty() {
                install_tools(rest, a.dry_run, a.native, h.config_path)?;
            }
            Ok(())
        }),
        Command::Update { check } => {
            if !check {
                with_host(
                    &builtin_commands(),
                    true,
                    shellcraft_plugins_cli::update_all,
                )?;
            }
            update_self(check);
            Ok(())
        }
        Command::Remove { name } => with_host(&builtin_commands(), true, |h| {
            shellcraft_plugins_cli::run(PluginCmd::Remove { name }, h)
        }),
        Command::List => with_host(&builtin_commands(), false, |h| {
            shellcraft_plugins_cli::run(PluginCmd::List, h)
        }),
        Command::Search { query } => with_host(&builtin_commands(), false, |h| {
            shellcraft_plugins_cli::run(PluginCmd::Search { query }, h)
        }),
        Command::Setup => wizard::run(
            &config_path()?,
            &mut std::io::stdin().lock(),
            &mut std::io::stdout(),
            std::env::var("SHELL").ok(),
        ),
        #[cfg(feature = "coreutils")]
        Command::Cheat(a) => shellcraft_coreutils::cheat::run(a),
        #[cfg(feature = "coreutils")]
        Command::Cat(a) => shellcraft_coreutils::cat::run(a),
        #[cfg(feature = "coreutils")]
        Command::Ls(a) => shellcraft_coreutils::ls::run_args(a),
        #[cfg(feature = "coreutils")]
        Command::Files(a) => shellcraft_coreutils::files::run(a),
        Command::Pick(a) => shellcraft_picker::run(a),
        Command::Plugin { cmd } => with_host(&builtin_commands(), true, |h| {
            shellcraft_plugins_cli::run(cmd, h)
        }),
        Command::Registry { cmd } => with_host(&builtin_commands(), false, |h| {
            shellcraft_plugins_cli::run_registry(cmd, h)
        }),
        Command::Pull(a) => with_host(&builtin_commands(), true, |h| {
            shellcraft_profile::pull(a, h)
        }),
        Command::Push(a) => with_host(&builtin_commands(), true, |h| {
            shellcraft_profile::push(a, h)
        }),
        Command::Hook(a) => shellcraft_plugins_cli::run_hooks(a),
        Command::External(args) => with_host(&visible_commands(), false, |h| {
            shellcraft_plugins_cli::run_external(&args, h)
        }),
        Command::Complete(a) => shellcraft_complete::run(a, &config_path()?),
        Command::Hist { cmd } => shellcraft_history::run(cmd),
        Command::Jump { cmd } => shellcraft_jump::run(cmd),
        Command::Path => {
            println!("{}", config_path()?.display());
            Ok(())
        }
    }
}
