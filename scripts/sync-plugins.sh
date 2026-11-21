#!/usr/bin/env bash
# Vendor a copy of the official plugins into ./vendored-plugins (what gets embedded in the binary).
#   scripts/sync-plugins.sh ../shellcraft-plugins        a local checkout (use a tagged commit)
# The plugins repository is the only place plugins are edited; this folder is replaced wholesale,
# never edited by hand. Every plugin's content hash is checked against the index after the copy.
set -euo pipefail
cd "$(dirname "$0")/.."
src=${1:?usage: scripts/sync-plugins.sh <path to a shellcraft-plugins checkout>}
[ -f "$src/index.toml" ] && [ -d "$src/plugins" ] || { echo "$src is not a shellcraft-plugins checkout" >&2; exit 1; }
rev=$(git -C "$src" describe --tags --always --dirty 2>/dev/null || echo unknown)
case $rev in *-dirty) echo "refusing: $src has uncommitted changes" >&2; exit 1 ;; esac
rm -rf vendored-plugins
mkdir -p vendored-plugins
cp "$src/index.toml" vendored-plugins/index.toml
[ ! -f "$src/index.toml.sig" ] || cp "$src/index.toml.sig" vendored-plugins/index.toml.sig
cp -R "$src/plugins" vendored-plugins/plugins
printf '%s\n' "$rev" > vendored-plugins/SOURCE
find vendored-plugins -name .DS_Store -delete
echo "vendored $rev from $src"
echo "now: cargo test -p shellcraft --test registry   (installs every plugin and checks its hash)"
