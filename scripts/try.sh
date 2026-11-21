#!/usr/bin/env bash
# Try shellcraft in a throw-away shell: ./scripts/try.sh [zsh|bash|fish|nu]
# Builds the debug binary, writes a default config and the init script into ./.sandbox,
# then starts the shell with only that script loaded. Your real rc files are not read or changed.
set -euo pipefail
cd "$(dirname "$0")/.."
shell=${1:-zsh}
export PATH="$HOME/.cargo/bin:/opt/homebrew/bin:$PATH"
cargo build --quiet
export PATH="$PWD/target/debug:$PATH"
sb="$PWD/.sandbox"
export XDG_CONFIG_HOME="$sb/config" XDG_DATA_HOME="$sb/data" XDG_CACHE_HOME="$sb/cache"
mkdir -p "$sb"
if [ ! -f "$XDG_CONFIG_HOME/shellcraft/shellcraft.toml" ]; then
  shellcraft init >/dev/null
fi
shellcraft apply --shell "$shell" >/dev/null
init="$XDG_CACHE_HOME/shellcraft/init.$shell"
[ "$shell" = nu ] && init="$XDG_CACHE_HOME/shellcraft/init.nu"
echo "sandbox: $sb  (config: $XDG_CONFIG_HOME/shellcraft/shellcraft.toml)"
case $shell in
  zsh)  mkdir -p "$sb/zdot"; printf 'source %q\n' "$init" > "$sb/zdot/.zshrc"; ZDOTDIR="$sb/zdot" exec zsh -i ;;
  bash) printf 'source %q\n' "$init" > "$sb/bashrc"; exec bash --rcfile "$sb/bashrc" -i ;;
  fish) exec fish -i --no-config -C "source $init" ;;
  nu)   printf 'source %s\n' "$init" > "$sb/config.nu"; : > "$sb/env.nu"; exec nu --config "$sb/config.nu" --env-config "$sb/env.nu" ;;
  *) echo "usage: $0 [zsh|bash|fish|nu]" >&2; exit 2 ;;
esac
