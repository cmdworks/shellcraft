# Commands

`shellcraft --help` and `shellcraft <command> --help` are always current; this is the overview.

## Setup and health

| Command | What it does |
| :--- | :--- |
| `init [--force]` | write a commented default config |
| `setup` | ask a few questions, write the config, apply it |
| `apply [--shell S] [--print \| --rc]` | generate the init script and cached tables; `--rc` adds a marked block to your rc file (with a backup); `--print` shows the script |
| `path` | print the config path (`SHELLCRAFT_CONFIG` overrides it; `XDG_CONFIG_HOME` is honoured) |
| `doctor` | check config, tools, generated script, rc wiring |
| `stats`, `bench [N] [--shell S]`, `profile` | disk use and history; shell startup time with and without your setup; zsh `zprof` |
| `uninstall [--purge]` (or the one-line `uninstall.sh`, see the README) | remove the rc block and generated files (the config is kept; `--purge` removes data too) |
| `completions <shell>` | completions for shellcraft itself |
| `shellcraft` (no arguments, in a terminal) / `tui` | full-screen manager for features, plugins and themes |

## Plugins (Homebrew-style)

| Command | What it does |
| :--- | :--- |
| `search [query]` | list the official plugins and plugins from your registries |
| `install <name>...` | install plugins by name (official, a folder, `owner/repo[/folder][@rev]`, a git URL with optional `#folder`; see [plugin-distribution.md](plugin-distribution.md)); other names are tried as tools (`fzf`, `zoxide`, ...); no names installs the tools enabled in your config |
| `update [--check]` | update every installed plugin, then shellcraft itself (not when a package manager installed it); `--check` only looks for a newer shellcraft |
| `remove <name>` | remove a plugin |
| `list` | bundled and installed plugins |
| `plugin add\|update\|remove\|list\|search\|info\|link\|new\|check\|slots` | the full plugin commands (`add` shows a summary and asks; `link` uses a folder live while you develop) |
| `registry add\|remove\|list\|update\|init\|publish\|keygen\|sign\|trust` | registries: where names are looked up, signing |
| `pull <repo>` / `push -m "..."` | restore a whole setup (config and pinned plugins) from a git repository, or save it back |

Install flags for tools: `--dry-run`, `--native` (download a checksummed release instead of using a package manager).

## Daily use

| Command | What it does |
| :--- | :--- |
| `j <words>`, `ji <words>` (shell functions) | jump to a frecent directory; `ji` picks from a list. Backed by `jump add\|query\|list\|remove\|prune` |
| `hist add\|search\|list\|stats\|suggest\|recent` | command history (Ctrl-R); `suggest` and `recent` feed the suggestion line |
| `theme [name]` | list themes or switch one (recolours prompt, fzf, bat) |
| `cheat [topic]` | cheat sheets (`shellcraft` is built in, the rest come from the `cheats` plugin) |
| `pick [--query Q] [--filter Q]` | fuzzy picker for lines on stdin |
| `files [path] [--dirs]` | list files or directories, honouring `.gitignore` |
| `ls`, `cat` | directory listing and file viewer (shell aliases `ls`, `ll`, `la`, `lt`, `cat`) |
| `suggest <word>` | closest command names in PATH (powers the command-not-found hint and `fix`) |
| `prompt --shell S [--segments-only]` | render the built-in prompt (the shells call this) |
| `complete` | completion candidates learned from `--help` (the shells call this) |

Commands added by plugins appear as `shellcraft <name>`.
