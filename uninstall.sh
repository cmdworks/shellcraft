#!/bin/sh
# Remove shellcraft: the rc wiring, generated files and the binary.
#   sh uninstall.sh          keep your config, history and installed plugins
#   sh uninstall.sh --all    delete those too
set -eu
bin=$(command -v shellcraft 2>/dev/null || true)
if [ -n "$bin" ]; then
  shellcraft uninstall --purge || echo "shellcraft uninstall failed; continuing" >&2
fi
if [ "${1:-}" = "--all" ]; then
  for d in "${XDG_CONFIG_HOME:-$HOME/.config}/shellcraft" "${XDG_DATA_HOME:-$HOME/.local/share}/shellcraft" "${XDG_CACHE_HOME:-$HOME/.cache}/shellcraft"; do
    case $d in */shellcraft) [ ! -d "$d" ] || { rm -rf "$d"; echo "removed $d"; } ;; esac
  done
fi
[ -z "$bin" ] || { rm -f "$bin" && echo "removed $bin"; }
echo "done; open a new terminal"
