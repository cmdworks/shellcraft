# Shell support

Tested means driven in a real shell in a pty (`scripts/sweep/run.py`) on macOS, Apple silicon. Linux
and Windows are untested.

| Feature | zsh | bash | fish | nushell | PowerShell |
| :--- | :---: | :---: | :---: | :---: | :---: |
| Init script, config, aliases, env, keys | yes | yes | yes | yes | yes |
| Built-in prompt | yes | yes | yes | yes | yes |
| Prompt drawn in the shell, segments refreshed in the background | yes | no | yes | no | no |
| `j`, `ji`, history, Ctrl-R, Ctrl-T, Alt-C | yes | yes | yes | yes | yes |
| Suggestion from history while typing | yes (computed in the shell) | no | fish's own | no | PSReadLine prediction |
| Syntax highlighting | yes | no | fish's own, themed | nushell's own | PSReadLine's own, themed |
| Command-not-found hint and `fix` | yes | hint only | yes | hint | no |
| Abbreviations | yes | no | yes | no | no |
| Flag and subcommand completion from `--help` | yes | no | no | no | no |
| Plugin hooks (`cd`, `post`) | yes | yes | yes | no | no |
| Tested | yes | yes | yes | yes (0.117) | not in recent changes |

Why some cells are "no": bash and nushell have no way to repaint a finished prompt from a background
process, and bash cannot draw text after the cursor, so those features would need a different
design, not a bug fix. PowerShell's support is limited by not being installed in the development
environment.

Per-shell notes live next to the code in `shells/<shell>/README.md`.
