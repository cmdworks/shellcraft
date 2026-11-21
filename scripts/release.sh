#!/usr/bin/env bash
# Build release assets in ./dist for one target (default: this machine):
#   scripts/release.sh [target-triple]        then upload dist/* to a GitHub release
#   scripts/release.sh formula                write dist/shellcraft.rb from the tarballs in dist/
# Assets (what install.sh, `shellcraft update` and the Homebrew formula expect):
#   shellcraft-<triple>.tar.gz   one file inside: shellcraft
#   shellcraft-<triple>.tar.gz.sha256
#   version.txt                  the version, one line
# To release all four targets build each on a matching machine (or with `cross`) and copy the
# tarballs into one dist/ folder, then run `scripts/release.sh formula`.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:/opt/homebrew/bin:$PATH"
dist=dist
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
sha() { if command -v sha256sum >/dev/null; then sha256sum "$1" | cut -d' ' -f1; else shasum -a 256 "$1" | cut -d' ' -f1; fi; }

if [ "${1:-}" = formula ]; then
  base="https://github.com/cmdworks/shellcraft/releases/download/v#{version}"
  out="$dist/shellcraft.rb"
  {
    echo "class Shellcraft < Formula"
    echo '  desc "Fast single-binary shell customization tool"'
    echo '  homepage "https://github.com/cmdworks/shellcraft"'
    echo "  version \"$version\""
    echo '  license "MIT"'
    block() { # os cpu triple
      f="$dist/shellcraft-$3.tar.gz"; [ -f "$f" ] || return 0
      echo "  on_$1 do"; echo "    on_$2 do"
      echo "      url \"$base/shellcraft-$3.tar.gz\""; echo "      sha256 \"$(sha "$f")\""
      echo "    end"; echo "  end"
    }
    block macos arm aarch64-apple-darwin
    block macos intel x86_64-apple-darwin
    block linux arm aarch64-unknown-linux-musl
    block linux intel x86_64-unknown-linux-musl
    echo '  def install'; echo '    bin.install "shellcraft"'; echo '  end'
    echo '  test do'; echo '    assert_match version.to_s, shell_output("#{bin}/shellcraft --version")'; echo '  end'
    echo "end"
  } > "$out"
  echo "wrote $out (put it in a tap: github.com/cmdworks/homebrew-tap/Formula/shellcraft.rb)"
  exit 0
fi

target=${1:-$(rustc -vV | sed -n 's/^host: //p')}
# release assets use musl names on Linux (static binaries)
asset=$target
case $target in *-linux-gnu) asset=${target%-gnu}-musl ;; esac
if [ "$asset" != "$target" ]; then
  echo "note: building $target; for a portable Linux asset build with --target $asset" >&2
fi
cargo build --release --locked --target "$target"
mkdir -p "$dist"
tmp=$(mktemp -d)
cp "target/$target/release/shellcraft" "$tmp/shellcraft"
tar -C "$tmp" -czf "$dist/shellcraft-$asset.tar.gz" shellcraft
sha "$dist/shellcraft-$asset.tar.gz" > "$dist/shellcraft-$asset.tar.gz.sha256"
echo "$version" > "$dist/version.txt"
rm -rf "$tmp"
ls -l "$dist"
echo "upload with: gh release create v$version $dist/* --title v$version"
