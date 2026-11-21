# Changelog

shellcraft has no release yet. The zshkit 0.1 and 0.2 history is in git before the Rust rewrite.

## Unreleased
- Plugins: sources can name a folder of a repository (`owner/repo/folder`, `url#folder`); `install` and `update` read the official plugins repository (cached shallow clone, refreshed daily, newest version wins over the embedded copy); a registry can live in a folder of a repository; installs from git registries record where they came from. `SHELLCRAFT_OFFLINE=1` stops all network use. GitHub Actions: ci and release workflows.
- Repository split: the official plugins are developed in cmdworks/shellcraft-plugins and a tagged copy is embedded (`vendored-plugins/`, `scripts/sync-plugins.sh`). New docs set (`docs/`), plugin manifest JSON schema, CONTRIBUTING and SECURITY.
- Install and update: `install.sh` (checked download, default config, shell wired, PATH handled by the init script), `shellcraft update` also updates shellcraft itself (`--check` only looks), `scripts/release.sh` builds release assets and a Homebrew formula.
- Homebrew-style plugin commands: `install`, `update`, `remove`, `list`, `search`; the official plugins are built into the binary (no registry to add).
- zsh: suggestions, highlighting and the prompt run inside the shell (typing 1.6 ms, prompt 4 ms, first Tab 53 ms); fish prompt too; history on SQLite with suggestions from other terminals; signed registries; theme and cheat plugins.
- Rust rewrite: config, five shell emitters, prompt, jump, history, picker, `ls`, `cat`, themes, abbreviations, suggestions, plugins (git, versions, docker, installable), completion learned from `--help`, `install`, `setup`, `doctor`, `stats`, `bench`.
