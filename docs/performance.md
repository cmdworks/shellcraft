# Performance

All numbers: macOS, Apple silicon, zsh 5.9, release build, median of many runs, interleaved between
setups. One machine, one run of the script; treat them as indicative. Reproduce:

```bash
git clone --depth 1 https://github.com/ohmyzsh/ohmyzsh.git omz/ohmyzsh
mkdir -p omz/custom/plugins
git clone --depth 1 https://github.com/zsh-users/zsh-autosuggestions.git omz/custom/plugins/zsh-autosuggestions
git clone --depth 1 https://github.com/zsh-users/zsh-syntax-highlighting.git omz/custom/plugins/zsh-syntax-highlighting
cargo build --release
python3 scripts/bench/compare-omz.py --omz ./omz
```

Setups: an empty `.zshrc`; Oh My Zsh with no plugins; Oh My Zsh with git, z, zsh-autosuggestions and
zsh-syntax-highlighting; shellcraft with its default config. Update checks are off and each setup has
its own completion cache.

| Measure (ms) | Oh My Zsh, no plugins | Oh My Zsh with plugins | shellcraft |
| :--- | ---: | ---: | ---: |
| Startup cost above an empty zsh | 24.6 | 47.3 | 6.2 |
| Enter on an empty line, in a git repo: you can type / fully drawn | 10.8 / 13.5 | 30.0 / 31.6 | 4.1 / 4.5 |
| `cd` between a repo and another directory: you can type / fully drawn | 17.9 / 52.4 | 58.1 / 75.7 | 29.3 / 36.0 |
| Typing: key to echo, suggestion and highlighting on | 0.7 | 1.5 | 1.7 |
| First Tab on `git sta`, after one command | 169.4 | 159.8 | 53.1 |

Other shells (`shellcraft bench`, startup cost above the shell's own): bash 2.2 ms, fish 3.0 ms.

## What the numbers do not say

- The `cd` rows include the cost of pasting the command through each setup's line editor.
- shellcraft's prompt also shows language versions and a dirty marker; Oh My Zsh's default does not.
- The history used is small and synthetic; a real one may behave differently.
- bash, nushell and PowerShell prompts still start the renderer on every prompt (about 17 ms in bash).
- Fish, bash and nushell are not compared with Oh My Zsh because it is zsh only.

## Budgets

Shell startup overhead under 10 ms, prompt render under 5 ms, jump lookup under 3 ms, binary under
10 MB (8.1 MB now). `shellcraft bench` checks the first; the others are measured in the same script.
