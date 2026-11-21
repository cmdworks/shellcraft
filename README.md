# shellcraft

A fast, single-binary shell customization tool written in Rust. One config drives zsh, bash, fish,
nushell and PowerShell; the prompt, directory jumping, history with suggestions, fuzzy picker,
`ls`, `cat` and completion are built in, and everything else is a plugin.

```bash
curl -fsSL https://raw.githubusercontent.com/cmdworks/shellcraft/main/install.sh | sh
```

That downloads a checked release, writes a default config and wires your shell (undo with
`shellcraft uninstall`). Open a new terminal and:

```bash
shellcraft search                  # the official plugins
shellcraft install docker themes   # install plugins by name; no configuration
shellcraft update                  # update shellcraft and every plugin
shellcraft doctor                  # check the setup
```

**Status: early development.** Tested on macOS (Apple silicon) with real zsh, bash, fish and nushell
sessions. Linux, Windows and PowerShell are untested. See [docs/shells.md](docs/shells.md) for exactly
what works where.

## Why

- **Fast.** In zsh, startup costs about 6 ms (Oh My Zsh with git, z, suggestions and highlighting:
  about 47 ms), typing is 1.7 ms per key, the prompt is usable 4 ms after Enter, the first Tab is
  about 50 ms (Oh My Zsh about 160 ms). Method and numbers: [docs/performance.md](docs/performance.md).
- **Self-contained.** No fzf, zoxide, starship, eza or bat needed. One 8 MB binary (4 MB minimal build).
- **One config.** `~/.config/shellcraft/shellcraft.toml`, validated, commented, the same for every shell.
- **Plugins with a contract.** A versioned plugin API, install by name, hash-locked, optionally signed
  registries. Official plugins live in [cmdworks/shellcraft-plugins](https://github.com/cmdworks/shellcraft-plugins).

## What you get

| | |
| :--- | :--- |
| Prompt | directory, git branch and dirty mark, language versions, duration, exit status; themes |
| Typing (zsh) | dim suggestion from your history (including your other terminals), syntax colours |
| Navigation | `j <words>` and `ji` (frecency jump), Ctrl-T files, Alt-C directories, built-in fuzzy picker |
| History | SQLite with exit code, directory and duration; Ctrl-R picker |
| Files | `ls` (colours, `-l`, tree), `cat` (syntax colours), cheat sheets |
| Completion | flags and subcommands learned from `--help`, cached |
| Plugins | prompt segments, commands, hooks, shell snippets, themes, cheat sheets, aliases, prebuilt binaries |
| Management | `setup`, `doctor`, `bench`, a full-screen manager (`shellcraft` with no arguments) |

## Documentation

| | |
| :--- | :--- |
| [docs/commands.md](docs/commands.md) | every command |
| [docs/configuration.md](docs/configuration.md) | every config option |
| [docs/shells.md](docs/shells.md) | what works in which shell |
| [docs/plugin-api.md](docs/plugin-api.md) | the plugin API (the contract), with a [JSON schema](docs/schema/plugin-manifest.schema.json) |
| [docs/plugin-distribution.md](docs/plugin-distribution.md) | where plugins come from (official, GitHub, any git URL), pinning, prebuilt binaries |
| [docs/architecture.md](docs/architecture.md) | how it works, where code goes |
| [docs/performance.md](docs/performance.md) | benchmarks and how to reproduce them |
| [docs/releasing.md](docs/releasing.md) | releases, install script, Homebrew, the plugins copy |
| [CONTRIBUTING.md](CONTRIBUTING.md), [SECURITY.md](SECURITY.md) | contributing and reporting problems |

Writing a plugin? Start with the guides in
[shellcraft-plugins](https://github.com/cmdworks/shellcraft-plugins) (`shellcraft plugin new`, `check`, `link`).

## Build from source

```bash
cargo build --release
target/release/shellcraft init && target/release/shellcraft apply --rc
```

`cargo build --release --no-default-features` builds the core without the TUI, tool installer and
`ls`/`cat`. Development: [CONTRIBUTING.md](CONTRIBUTING.md). A throw-away shell to try changes without
touching your own setup: `scripts/try.sh zsh` (or `direnv allow`, then `try-zsh`).

## Honest limits

No sandbox: plugins run as you (see [SECURITY.md](SECURITY.md)). Suggestions and highlighting exist
only in zsh (fish and PowerShell use their own, themed). bash gets the prompt, jump, history and
picker but no ghost text. Nothing here has had more than one person using it yet.

## License

MIT, see [LICENSE](LICENSE).
