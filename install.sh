#!/bin/sh
# Install shellcraft:  curl -fsSL https://raw.githubusercontent.com/cmdworks/shellcraft/main/install.sh | sh
#
# Downloads the release for this machine, checks its SHA-256, puts `shellcraft` in
# ~/.local/bin, creates a default config and wires your shell (undo with `shellcraft uninstall`).
#   SHELLCRAFT_VERSION=0.1.0   install that release instead of the latest
#   SHELLCRAFT_INSTALL_DIR=... where the binary goes (default ~/.local/bin)
#   SHELLCRAFT_NO_RC=1         install the binary only, do not touch your shell config
#   SHELLCRAFT_BASE_URL=...    release folder (https:// or file://), for mirrors and tests
set -eu

say() { printf '%s\n' "$*"; }
die() { printf 'shellcraft install: %s\n' "$*" >&2; exit 1; }

os=$(uname -s); arch=$(uname -m)
case "$os-$arch" in
  Darwin-arm64) triple=aarch64-apple-darwin ;;
  Darwin-x86_64) triple=x86_64-apple-darwin ;;
  Linux-x86_64) triple=x86_64-unknown-linux-musl ;;
  Linux-aarch64 | Linux-arm64) triple=aarch64-unknown-linux-musl ;;
  *) die "no release for $os $arch yet (build from source: cargo install --path cli)" ;;
esac

if [ -n "${SHELLCRAFT_BASE_URL:-}" ]; then
  base=$SHELLCRAFT_BASE_URL
elif [ -n "${SHELLCRAFT_VERSION:-}" ]; then
  base=https://github.com/cmdworks/shellcraft/releases/download/v${SHELLCRAFT_VERSION#v}
else
  base=https://github.com/cmdworks/shellcraft/releases/latest/download
fi
dir=${SHELLCRAFT_INSTALL_DIR:-$HOME/.local/bin}
asset=shellcraft-$triple.tar.gz

if command -v curl >/dev/null 2>&1; then
  get() { curl -fsSL "$1" -o "$2"; }
elif command -v wget >/dev/null 2>&1; then
  get() { wget -q "$1" -O "$2"; }
else
  die "need curl or wget"
fi
if command -v sha256sum >/dev/null 2>&1; then
  sum() { sha256sum "$1" | cut -d' ' -f1; }
elif command -v shasum >/dev/null 2>&1; then
  sum() { shasum -a 256 "$1" | cut -d' ' -f1; }
else
  die "need sha256sum or shasum to check the download"
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT INT TERM

say "downloading $asset"
get "$base/$asset" "$tmp/$asset" || die "could not download $base/$asset"
get "$base/$asset.sha256" "$tmp/$asset.sha256" || die "could not download the checksum"
want=$(cut -d' ' -f1 < "$tmp/$asset.sha256" | tr -d '[:space:]')
have=$(sum "$tmp/$asset")
[ "$want" = "$have" ] || die "checksum mismatch (expected $want, got $have); nothing installed"

tar -xzf "$tmp/$asset" -C "$tmp" shellcraft || die "unexpected archive layout"
mkdir -p "$dir"
cp "$tmp/shellcraft" "$dir/shellcraft.new"
chmod 755 "$dir/shellcraft.new"
mv "$dir/shellcraft.new" "$dir/shellcraft"
say "installed $("$dir/shellcraft" --version) to $dir/shellcraft"

case ":$PATH:" in
  *":$dir:"*) ;;
  *) say "note: $dir is not on your PATH; add it, for example: export PATH=\"$dir:\$PATH\"" ;;
esac

if [ "${SHELLCRAFT_NO_RC:-}" = 1 ]; then
  say "skipped shell setup; run \`shellcraft setup\` when you want it"
  exit 0
fi
cfg=$("$dir/shellcraft" path 2>/dev/null || true)
if [ -z "$cfg" ] || [ ! -f "$cfg" ]; then "$dir/shellcraft" init >/dev/null; fi
"$dir/shellcraft" apply --rc
say ""
say "done. Open a new terminal. Try: shellcraft search | shellcraft install docker | shellcraft update"
say "undo any time: shellcraft uninstall"
