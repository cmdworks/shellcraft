# Configuration

The file is `~/.config/shellcraft/shellcraft.toml` (`shellcraft path` prints it). Unknown keys are
errors, so typos are caught. After editing run `shellcraft apply` (the manager and the `install`,
`theme` and plugin commands do it for you). `shellcraft init` writes a commented default.

| Section | Key | Default | Meaning |
| :--- | :--- | :--- | :--- |
| `[shell]` | `default` | none | shell to generate for when none is given: `zsh`, `bash`, `fish`, `nu`, `pwsh` |
| | `history_size` | `50000` | lines of the shell's own history |
| `[tools]` | `starship`, `zoxide`, `fzf`, `mise`, `atuin` | off | wire the original tool in as well; shellcraft needs none of them |
| `[prompt]` | `builtin` | `true` | shellcraft's own prompt; `false` keeps starship if enabled |
| | `symbol` | `❯` | text before the cursor |
| | `min_duration_ms` | `2000` | show how long the last command took when it was slower than this |
| | `segments` | `["git", "versions"]` | prompt segments, in order: a plugin name (all its segments) or `plugin.segment` |
| `[theme]` | `name` | `default` | `default` is built in; others come from theme plugins |
| `[suggest]` | `inline` | `true` | dim suggestion from history after the cursor (zsh; PowerShell has its own prediction) |
| | `highlight` | `true` | colour the line while you type (zsh; fish and PowerShell use their own, themed) |
| `[completion]` | `help` | `true` | learn flags and subcommands from `<command> --help` (cached a week) |
| | `skip` | `[]` | commands whose `--help` is never run |
| `[registry]` | `sources` | `[]` | extra registries (folder, `owner/repo`, git URL); the official plugins need no entry |
| | `keys` | `[]` | trusted registry public keys (64 hex digits); once set, unsigned registries are ignored |
| `[plugins]` | `list` | `[]` | plugins as `owner/repo` (loaded by the shell's own plugin manager where one exists) |
| `[plugin.<name>]` | `enabled` | `true` | switch a plugin off without removing it |
| | `segment`, `ttl_ms` | | a prompt segment from your own executable: `segment = "/path/exe"` (prints one line) |
| | `options` | | free-form options for the plugin, for example `[plugin.versions.options] ecosystems = "node,go"` |
| `[aliases]` | `name = "expansion"` | | shell aliases |
| `[abbr]` | `name = "expansion"` | | expanded in place when you press space (zsh, fish) |
| `[env]` | `NAME = "value"` | | environment variables |
| `[keys]` | `"ctrl-r" = "widget"` | | key bindings |

Environment variables: `SHELLCRAFT_CONFIG` (config file), `XDG_CONFIG_HOME`, `XDG_DATA_HOME`,
`XDG_CACHE_HOME`, `SHELLCRAFT_OFFLINE=1` (never use the network: no official-repository refresh, no update check),
`SHELLCRAFT_UPDATE_URL` (where `update` looks for a newer shellcraft), `SHELLCRAFT_DEV_ARTIFACTS` (read plugin
binary files from a folder while developing).

Files and where they live:

| Path | Content |
| :--- | :--- |
| `$XDG_CONFIG_HOME/shellcraft/shellcraft.toml`, `shellcraft.lock` | your config; installed plugins with source, commit, content hash |
| `$XDG_DATA_HOME/shellcraft/` | `history.db`, `jump.tsv`, `plugins/<name>/`, `plugin-bin/` |
| `$XDG_CACHE_HOME/shellcraft/` | the generated init scripts, `config.bin` and other caches; safe to delete (`shellcraft apply` rebuilds them) |

Defaults for the three directories are `~/.config`, `~/.local/share` and `~/.cache`.
