# shellcraft plugin API, version 1

This is the contract between shellcraft and a plugin. Everything in this file is implemented and tested. Anything marked **planned** is not.

## Quick start

```bash
shellcraft plugin new mine --kind segment     # skeleton (also: snippet, command, hook, data)
shellcraft plugin check ./mine                # validate, syntax-check snippets, run segments once
shellcraft plugin link ./mine                 # use it from your folder while you develop
# edit ./mine/bin/segment: the prompt picks it up at once
# edit the manifest or a snippet: run `shellcraft apply`
shellcraft plugin remove mine                 # removes the link, never your folder
shellcraft plugin add ./mine                  # install a copy (pinned and hash-locked)
shellcraft plugin add owner/repo@v1.0.0       # or from git (https, ssh, file URLs, or a path)
```

A plugin is a folder (usually a git repository) with a `shellcraft-plugin.toml` and the files it points to.

## Manifest `shellcraft-plugin.toml`

Unknown fields are an error, so typos are caught early.

| Field | Type | Meaning |
| :--- | :--- | :--- |
| `api` | integer, default 1 | The plugin API version you wrote this for. A higher number than the installed shellcraft supports is refused. |
| `name` | string, required | Letters, digits, `-`, `_`. Not a bundled plugin's name (`git`, `versions`). |
| `version` | string, required | Free-form, shown to users. |
| `description`, `license` | string | Shown at install and in `plugin list`. |
| `shells` | list | Shells the plugin supports: `zsh`, `bash`, `fish`, `nu`, `pwsh`. Empty means all. |
| `[segment.<id>]` | table | A prompt segment (see below). |
| `[command.<name>]` | table | `shellcraft <name> ...` (see below). |
| `[hook.<id>]` | table | A program started on an event (see below). |
| `kind` | string, default `feature` | What the plugin is: `feature`, `segment`, `shell`, `theme`, `tool` or `profile`. Informational today; a `profile` plugin is just `requires`. |
| `requires` | list | Plugins needed first, e.g. `["base>=2.0", "git"]`. Bundled plugins count as installed. Checked at install. |
| `conflicts` | list | Plugins this one cannot be installed next to. |
| `[[slot]]` | array of tables | `shell`, `name`, `file`, `position = "before"\|"after"`: code attached to a named slot of a shell's init script (see Slots). |
| `[snippets]` | table | `<shell> = "path"`: code added to that shell's init script (the `plugin_snippets` slot, before the shell's own functions). |
| `[aliases]`, `[abbr]`, `[env]` | tables | Data added to the config; the user's own entries win. |
| `[theme.<name>]` | table | A colour theme: `dir`, `git`, `duration`, `ok`, `err`, `dim`, `version`, `tool` (SGR parameters such as `34` or `38;5;110`), `fzf`, `bat`, plus `[theme.<name>.layout]` (own prompt layout, icons, multi-colour symbol, right prompt) and `[theme.<name>.options.<plugin>]` (see the themes guide in the plugins repository). Fields left out keep the default theme's. Choose it with `shellcraft theme <name>`. `default` is reserved. |
| `[cheat.<topic>]` | table | `file = "cheats/x.txt"`: a cheat sheet shown by `shellcraft cheat <topic>`. |
| `[[binary]]` | array of tables | Prebuilt executables per platform (see Prebuilt binaries). |
| `[permissions]` | table | What the plugin says it does: `exec = [...]`, `network = bool`, `write = [...]`. Shown at install so you can decide. **Information only, not enforced**: plugins run as you. |

All file paths are relative to the plugin folder. Absolute paths, `~` and `..` are rejected.

### Segments

```toml
[segment.main]
command = "bin/segment"          # executable, no arguments
ttl_ms = 10000                   # how long its output is reused (default 10000)
when = { files = ["Cargo.toml"] } # only in projects containing one of these (searched upward, 8 levels)
```

The executable runs with the current directory as its working directory and prints **one line**. An optional tone in brackets selects the colour from the user's theme: `[git]`, `[version]`, `[tool]`, `[warn]`, `[ok]`, `[err]`, `[dim]` (default `[tool]`). Empty output shows nothing. Examples: `[warn] 3 todos`, `node 20`.

The prompt never waits for a segment. It shows the last cached line for the current directory; when that is older than `ttl_ms` it starts a background refresh (2 second limit). The first prompt in a new directory therefore shows nothing from your plugin. Keep segments fast anyway: `plugin check` reports the time and flags anything over 50 ms.

Place a segment with `[prompt] segments = ["myplugin"]` (all its segments) or `"myplugin.main"` (one). Installed plugins not listed there are appended after the listed ones; `[plugin.myplugin] enabled = false` switches one off.

### Commands

```toml
[command.todo]
run = "bin/todo"
about = "manage TODOs"
```

`shellcraft todo a b c` runs `bin/todo a b c` and exits with its status. The name may not be a built-in command or another plugin's command; install refuses it.

### Hooks

```toml
[hook.notify]
event = "post"                   # "cd" or "post"
run = "bin/notify"
```

Started in the background, detached, with these environment variables:

| Variable | `cd` | `post` |
| :--- | :--- | :--- |
| `SHELLCRAFT_EVENT` | `cd` | `post` |
| `SHELLCRAFT_CWD` | new directory | directory the command ran in |
| `SHELLCRAFT_OLDPWD` | previous directory | |
| `SHELLCRAFT_EXIT` | | exit status |
| `SHELLCRAFT_DURATION_MS` | | duration (not available in bash 3.2) |
| `SHELLCRAFT_COMMAND` | | the command line |

Hooks cannot block the prompt. They can pile up if they are slow. Hooks are wired only into zsh, bash and fish, and only when some installed plugin listens to the event.

### Slots

A shell's init script is an ordered list of slots (`options`, `env`, `completion`, `keybindings`, `tools`, `history`, `prompt`, `aliases`, `jump`, `functions`, ...). `shellcraft plugin slots zsh` prints them in order. A plugin attaches code *before* or *after* any slot of any shell:

```toml
[[slot]]
shell = "zsh"
name = "aliases"
file = "slots/pre-aliases.zsh"
position = "before"
```

The file's text is inserted unchanged. Plugins are applied in install order. A slot name the shell does not have is reported by `apply` and skipped, so a plugin written for a newer shellcraft degrades instead of breaking the script.

### Prebuilt binaries

A plugin written in Rust, Go or any compiled language ships executables per platform instead of building on the user's machine:

```toml
[[binary]]
name = "todo"                          # referenced as @todo
target = "aarch64-apple-darwin"
url = "https://github.com/acme/todo/releases/download/v1.0.0/todo-aarch64-apple-darwin.tar.gz"
sha256 = "…64 hex digits…"
archive = "tar.gz"                     # or "raw" (default)
member = "todo"                        # file inside the archive (default: the name)

[[binary]]
name = "todo"
target = "x86_64-unknown-linux-musl"
url = "…"
sha256 = "…"

[command.todo]
run = "@todo"                          # also valid in [segment.*] command and [hook.*] run
```

At install the build for the user's target is downloaded, its SHA-256 is checked, the file is unpacked into `<data>/shellcraft/plugin-bin/<plugin>/`, and the checksum of the installed file is recorded in `shellcraft.lock`. Later, `apply` re-checks that file; if it changed or is missing, the plugin is skipped with a warning. A plugin with no build for the user's platform is refused. The install summary lists every download.

For development, set `SHELLCRAFT_DEV_ARTIFACTS=<dir>`: a `url` that is a bare file name or `file://` path is then read from that directory (and only from it), with the same checksum check. Without the variable only `https://` is accepted. `SHELLCRAFT_DEV_TARGET` overrides the detected target triple.

### Requirements are installed for you

`plugin add <name>` first installs any missing `requires` that a configured registry can provide (asking for each unless `--yes`), up to four levels deep. A requirement no registry has is reported as before.

### Snippets

`[snippets] zsh = "snippets/init.zsh"` adds the file's text, unchanged and untemplated, to that shell's init script after the built-in functions. It runs inside the user's shell, so it can do anything the user can; keep it small and check it with `plugin check` (which runs `zsh -n`, `bash -n` or `fish -n` when those shells are installed).

## What shellcraft gives you, and what it promises

- **Install**: `plugin add` shows the manifest summary and asks. The commit and a SHA-256 over all files (excluding `.git`) are stored in `shellcraft.lock`. If the files change later, the plugin is skipped with a warning until it is updated or reinstalled.
- **Update**: `plugin update` fetches, shows a diff summary, asks; changed permissions are always asked, even with `--yes`. A plugin installed from `owner/repo@rev` is pinned and never updated.
- **Development**: `plugin link` skips hash checks and updates; it is for your own code.
- **Compatibility**: the API version is one integer. Adding optional manifest fields (such as `kind`, `requires` and `[[slot]]`), new slots or new tones does not change it. A breaking change raises it; shellcraft will keep supporting the previous version for at least one release. Pin your plugin with `api = 1`.
- **Safety**: plugins and their snippets run as you, with no sandbox. What protects you is the hash lock (a changed plugin is skipped until you update it), the permission summary at install, and, if you want it, signed registries. Anyone can publish a plugin; read before you install.

## Testing your plugin

- `shellcraft plugin check <dir>` validates the manifest, checks files exist and are executable, syntax-checks snippets, runs each segment once with timing.
- Run a real shell against your plugin: `./scripts/try.sh zsh` in the shellcraft repo starts a sandboxed shell; `plugin link` your folder first.
- Hooks and commands are plain executables: test them directly with the environment variables above.

## Distribution

A git repository whose root holds `shellcraft-plugin.toml`. Tag releases and tell users to install `owner/repo@v1.2.3`.

### Registries (install by name)

A registry is a git repository, or just a folder, with an `index.toml`:

```toml
[[plugin]]
name = "todos"
version = "1.0.0"
description = "count TODOs"
kind = "feature"
repo = "acme/shellcraft-todos"   # folder inside the registry, owner/repo, or a git URL
rev = "v1.0.0"                   # optional tag or commit
hash = "…"                       # SHA-256 of the files; verified at install
```

```bash
shellcraft registry init ./my-registry                         # create one
shellcraft registry publish ./my-registry ./todos              # add a plugin (records version, commit, hash)
shellcraft registry add ./my-registry                          # use it (also owner/repo or a git URL)
shellcraft plugin search todo
shellcraft plugin info todos
shellcraft plugin add todos                                    # resolved through the registries, hash checked
```

`[registry] sources = [...]` in the config lists them in order.

**Signed registries.** `shellcraft registry keygen ./secret.key` creates an Ed25519 key pair (the secret stays in the file, the public key is printed); `shellcraft registry sign ./my-registry --key ./secret.key` writes `index.toml.sig`; users run `shellcraft registry trust <public key>` (stored in `[registry] keys`). Once any key is trusted, a registry that is unsigned, edited after signing, or signed by another key is ignored with a warning. With no key configured, unsigned registries still work (development). The signature covers only `index.toml`; the per-plugin content hash in it covers the files. A plugin whose files do not match the pinned hash is refused.

### Profiles (a whole setup from a repository)

A profile is a git repository (or folder) holding `shellcraft.toml` and `shellcraft.lock`. `shellcraft pull <repo>` shows what it will do, backs up your config, replaces it, and installs exactly the plugins in the lock (same source, same commit, same content hash; plugins that need others are installed after them). `shellcraft push -m "message"` commits your config and lock back (plugins linked for development or installed from a folder are left out because other machines cannot reproduce them).

### Without GitHub

Everything above works with local folders and `file://` URLs. The tests also run the real `owner/repo` path without the network: git rewrites `https://github.com/` to a local directory (`GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=url.file:///path/to/hub/.insteadOf GIT_CONFIG_VALUE_0=https://github.com/`), so the code builds the genuine GitHub URL and git serves it from `hub/<owner>/<repo>.git`. You can use the same trick to develop against fake repositories.

## Manager UI

`shellcraft` with no arguments on a terminal (or `shellcraft tui`) opens a full-screen manager: features, plugin browser (installed plus registry entries, mark to install or remove), theme preview, a diff review, then apply. It uses the same install and remove code as `plugin add|remove`.

## Planned (not in API 1)

- A shell pack (`kind = "shell"`) living outside the repository: the five shells are still compiled in.

## Stability and versioning

- The API version is the `api` integer in the manifest (today `1`). Adding optional fields, new slots,
  new tones or new kinds never changes it. A breaking change raises it, and shellcraft keeps
  supporting the previous version for at least one release.
- A plugin that asks for a higher `api` than the installed shellcraft supports is refused with a
  message telling the user to update shellcraft.
- The machine-readable shape of the manifest is
  [schema/plugin-manifest.schema.json](schema/plugin-manifest.schema.json); `shellcraft plugin check` is
  the reference validator and wins if the two ever disagree.

## Where to learn more and where plugins live

Step-by-step guides, one example per plugin kind, the review checklist and the official plugins are in
[cmdworks/shellcraft-plugins](https://github.com/cmdworks/shellcraft-plugins). The official plugins also
ship inside the binary, so `shellcraft install <name>` needs no registry.
