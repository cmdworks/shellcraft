<div align="center">

# ⚡ shellcraft

**A fast, single-binary shell customization tool.**<br>
One config for zsh, bash, fish, nushell and PowerShell. Plugins and themes included.

[![release](https://img.shields.io/github/v/release/cmdworks/shellcraft?include_prereleases&label=release&color=orange)](https://github.com/cmdworks/shellcraft/releases)
[![CI](https://img.shields.io/github/actions/workflow/status/cmdworks/shellcraft/ci.yml?label=CI)](https://github.com/cmdworks/shellcraft/actions/workflows/ci.yml)
[![license: MIT](https://img.shields.io/badge/license-MIT-blue)](LICENSE)
![built with Rust](https://img.shields.io/badge/built%20with-Rust-orange?logo=rust)
![shells](https://img.shields.io/badge/shells-zsh%20·%20bash%20·%20fish%20·%20nushell%20·%20pwsh-informational)
[![plugins](https://img.shields.io/badge/plugins-shellcraft--plugins-8A2BE2)](https://github.com/cmdworks/shellcraft-plugins)

[Install](#-install) · [Features](#-features) · [Commands](#-commands) · [Plugins and themes](#-plugins-and-themes) · [Docs](#-documentation) · [Plugins repo](https://github.com/cmdworks/shellcraft-plugins)

</div>

```text
…/code/app  main ✔  rs stable ❯❯❯                                         [󰎫 1.2s]
```

<sub>The `passionp` theme (needs a Nerd Font): icons, a red-yellow-green arrow, command time on the right.</sub>

---

## 🚀 Install

```bash
curl -fsSL https://raw.githubusercontent.com/cmdworks/shellcraft/main/install.sh | sh
```

Downloads a checked release into `~/.local/bin`, writes a default config and wires your shell.
Open a new terminal and you are done. Or build it yourself: `cargo build --release`.

```bash
shellcraft doctor                  # check the setup
shellcraft install docker nord      # add plugins by name, no configuration
shellcraft theme passionp          # switch theme
shellcraft update                  # update shellcraft and every plugin
```

> **Status: early development (alpha).** Tested on macOS (Apple silicon) with real zsh, bash, fish
> and nushell sessions. Linux, Windows and PowerShell are covered by CI only.
> [docs/shells.md](docs/shells.md) says exactly what works where.

## ✨ Features

<table>
<tr>
<td width="50%" valign="top">

**🎯 Prompt**<br>
Directory, git branch and dirty mark, language versions, command time, exit status. Themes can
define the whole layout: icons, multi-colour symbol, a right-hand prompt.

**🧭 Navigation**<br>
`j <words>` and `ji` (frecency jump), Ctrl-T files, Alt-C directories, a built-in fuzzy picker.
No fzf or zoxide needed.

**📜 History**<br>
SQLite with exit code, directory and duration. Ctrl-R picker; dim suggestions from your history,
including your other terminals (zsh).

</td>
<td width="50%" valign="top">

**🎨 Themes**<br>
`nord`, `dracula`, `gruvbox`, `passionp` and your own. They recolour the prompt, fzf, bat and
shell highlighting.

**🧩 Plugins with a contract**<br>
Prompt segments, commands, hooks, shell snippets, themes, cheat sheets, aliases, prebuilt binaries.
Versioned API, install by name, hash-locked.

**🗂️ Files and completion**<br>
Built-in `ls` (colours, tree) and `cat` (syntax colours), cheat sheets, completion learned from
`--help` and cached.

</td>
</tr>
</table>

| | shellcraft | Oh My Zsh (git, z, suggestions, highlighting) |
| :--- | :--- | :--- |
| Shell startup cost (zsh) | **about 6 ms** | about 47 ms |
| First Tab press | **about 50 ms** | about 160 ms |
| Typing latency | 1.7 ms per key | n/a |
| Prompt usable after Enter | about 4 ms | n/a |
| Footprint | one 8 MB binary (4 MB minimal build) | a framework plus plugin clones |

<sub>macOS Apple silicon, zsh 5.9. Method and raw numbers: [docs/performance.md](docs/performance.md).</sub>

## 🛠 Commands

`shellcraft --help` is always current; the full reference is [docs/commands.md](docs/commands.md).

**Set up and look after**

| Command | What it does |
| :--- | :--- |
| `shellcraft` | full-screen manager: features, plugins, themes |
| `setup` / `init` | a short wizard, or just write a commented default config |
| `apply [--rc]` | generate the shell script (and wire your rc file, with a backup) |
| `doctor` · `stats` · `bench` | check the setup, disk use, startup time |
| `update [--check]` | update every plugin, then shellcraft itself |
| `uninstall [--purge]` | remove the wiring and generated files (config is kept) |

**Plugins and themes**

| Command | What it does |
| :--- | :--- |
| `search [query]` | the official plugins and your registries |
| `install <name>...` | by name, `owner/repo[/folder][@rev]`, or a git URL (`#folder`) |
| `remove <name>` · `list` | remove, or list what is installed |
| `theme [name]` | list themes or switch one |
| `plugin new · check · link` | write, validate and live-link your own plugin |
| `pull <repo>` · `push` | restore or save your whole setup through a git repository |

**Every day**

| Command | What it does |
| :--- | :--- |
| `j <words>` · `ji` | jump to a frecent directory (`ji` shows a list) |
| `hist search · stats` | search and summarise your history |
| `cheat [topic]` | cheat sheets (git, tar, ssh, ...) |
| `pick` · `files` | fuzzy picker for any list; files that honour `.gitignore` |
| `ls` · `cat` | built-in directory listing and file viewer |

## 🧩 Plugins and themes

The official plugins and themes live in **[cmdworks/shellcraft-plugins](https://github.com/cmdworks/shellcraft-plugins)**
and also ship inside the binary, so `install` works offline. Anyone can publish one: from a GitHub
repository, any git URL, or a folder of a repository.

```bash
shellcraft install docker git-aliases cheats     # official plugins
shellcraft install passionp && shellcraft theme passionp
shellcraft install someone/their-plugin@v1.2     # your own or a community plugin
```

Plugins run as you and are not sandboxed: read before you install ([SECURITY.md](SECURITY.md)).
Writing one? Start with the guides in the plugins repository, then the [plugin API](docs/plugin-api.md).

## 🧹 Update and uninstall

```bash
shellcraft update
shellcraft uninstall                 # keeps your config
```

Remove everything (wiring, binary, config, history, plugins) in one line:

```bash
curl -fsSL https://raw.githubusercontent.com/cmdworks/shellcraft/main/uninstall.sh | sh -s -- --all
```

Leave off `-- --all` to keep your config and data. The script is [uninstall.sh](uninstall.sh), a
few lines you can read first.

## 📚 Documentation

| | |
| :--- | :--- |
| [Commands](docs/commands.md) | every command |
| [Configuration](docs/configuration.md) | every config option |
| [Shells](docs/shells.md) | what works in which shell |
| [Plugin API](docs/plugin-api.md) | the contract, with a [JSON schema](docs/schema/plugin-manifest.schema.json) |
| [Plugin distribution](docs/plugin-distribution.md) | official, GitHub, any git URL; pinning; prebuilt binaries |
| [Architecture](docs/architecture.md) | how it works, where code goes |
| [Performance](docs/performance.md) | benchmarks and how to reproduce them |
| [Releasing](docs/releasing.md) | releases, install script, Homebrew |
| [Contributing](CONTRIBUTING.md) · [Security](SECURITY.md) | contributing, reporting problems |

## 🔧 Build from source

```bash
cargo build --release
target/release/shellcraft init && target/release/shellcraft apply --rc
```

`cargo build --release --no-default-features` builds the core without the TUI, tool installer and
`ls`/`cat`. A throw-away shell to try changes without touching your own setup: `scripts/try.sh zsh`.

## ⚠️ Honest limits

No sandbox: plugins run as you. Suggestions and highlighting exist only in zsh (fish and PowerShell
use their own, themed); bash gets the prompt, jump, history and picker but no ghost text. Layout
themes (right prompt) work in zsh and fish. Nothing here has had more than one person using it yet.

## 🤝 Contributing and license

Issues and pull requests are welcome: [CONTRIBUTING.md](CONTRIBUTING.md). MIT licensed,
see [LICENSE](LICENSE). Plugins and themes: [cmdworks/shellcraft-plugins](https://github.com/cmdworks/shellcraft-plugins).
