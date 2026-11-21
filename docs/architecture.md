# Architecture

## The idea

`shellcraft apply` turns your config and your plugins into one static init script per shell. The shell
sources that script; afterwards the shell never reads TOML or talks to a plugin host. The only things
that run later are small, fast commands that read caches.

```text
shellcraft.toml ──apply──> init script per shell (static text)      <cache>/init.<shell>
                      ├──> config.bin (postcard)                    read by prompt, complete, plugin
                      └──> plugins.tsv, commands.tsv, hooks.tsv,    baked from installed plugins
                           cheats.tsv
the shell sources the init script; its hooks then call
  shellcraft prompt | jump | hist | hook | complete | pick | files
```

Three kinds of work, with different rules:

| Kind | When | Rule |
| :--- | :--- | :--- |
| Generate | `apply`, installs, the manager, `pull` | may be slow and use the network; output is static files |
| Hot | every prompt, `cd`, Ctrl-R, Tab | a short process that reads caches only: no TOML, no network, no async runtime |
| Per keystroke (zsh) | while typing | no process at all: the init script keeps what it needs in shell memory |

Budgets: shell startup overhead under 10 ms, prompt render under 5 ms, jump lookup under 3 ms, binary
under 10 MB. `shellcraft bench` and [performance.md](performance.md) measure them.

## Workspace

```text
core/           config schema and validation, themes, paths, cache. No shell, OS, plugin or feature knowledge
platform/       everything that differs by OS (release triples, Homebrew, package managers)
plugin-host/    plugin API (Plugin trait, Ctx, Segment), manifest, lock and store, outside segments
bundled/        first-party plugins compiled in (git, versions) and the embedded official-plugins copy
shells/         one crate per shell (emitter, templates with a partial per feature) and shells/api, shells/all
features/       one crate per feature: logic, clap arguments, tests (jump, history, prompt, complete, picker,
                coreutils, installer, health, plugins-cli, profile, tui, not-found)
cli/            the thin binary: command tree, dispatch, setup commands
vendored-plugins/   a copy of the shellcraft-plugins repository, embedded in the binary
docs/ scripts/      documentation; try.sh, sweep (real-shell checks), bench (comparison), release scripts
```

Dependencies flow one way: `core`, `platform` ← `plugin-host` ← `bundled/*`, `shells/*` ← `features/*` ← `cli`.
Shell crates know nothing about features or plugins; `apply` attaches plugin snippets and data to the
config before rendering.

## Where does new code go?

| I want to... | Put it in |
| :--- | :--- |
| support another shell | a new `shells/<name>/` crate (copy one), register it in `shells/all` and `core::Shell` |
| change what one shell does for a feature | the feature's partial: `shells/<shell>/templates/<feature>.j2` |
| add a config option | `core/src/config.rs` (struct, default, validation), `default_config.toml`, bump `cache::FORMAT`, `docs/configuration.md` |
| add a command | a crate under `features/` with its clap args and `run()`, one variant in `cli/src/main.rs`, an integration test in `cli/tests/`, a row in `docs/commands.md` |
| add something OS specific | `platform/src/<os>.rs` |
| add a prompt segment or other extra | a plugin (`shellcraft plugin new`), in the shellcraft-plugins repository if it should be official |

## Templates

Each shell has `templates/main.j2` plus one partial per feature, rendered with minijinja. Rules that
keep the output stable: a partial ends with one newline; do not start one with `{%-` or end it with
`-%}` (whitespace control does not cross files); text containing `{#` needs `{% raw %}`. Snapshots in
`shells/all/tests/snapshots/` pin the exact output of every shell.

## Plugins

The plugin API is [plugin-api.md](plugin-api.md). First-party plugins use the same interface as
installed ones; the official plugins are developed in
[shellcraft-plugins](https://github.com/cmdworks/shellcraft-plugins) and a tagged copy is embedded
(`vendored-plugins/`, synced by `scripts/sync-plugins.sh`), so `shellcraft install <name>` works offline; a shallow
clone of that repository is read when it is cached and refreshed daily (see [plugin-distribution.md](plugin-distribution.md)).

## Dependencies

postcard (config cache), clap, minijinja (templates), nucleo-matcher and crossterm (picker), ratatui
(manager), ignore (file walker), syntect (`cat`), rusqlite with bundled SQLite (history), ureq, sha2,
flate2 and tar (downloads), ed25519-dalek (registry signatures), insta and criterion (tests and
benchmarks). New dependencies need a one-line reason and license in the pull request that adds them.
Release profile: `opt-level=3`, `lto="fat"`, `codegen-units=1`, `panic="abort"`, `strip=true`.
