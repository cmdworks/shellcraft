# Releasing

## shellcraft

Pushing a tag `vX.Y.Z` (matching `Cargo.toml`) runs `.github/workflows/release.yml`: it builds the four
assets on matching runners, then publishes the GitHub release with `version.txt` and the Homebrew
formula. `.github/workflows/ci.yml` runs fmt, clippy, tests and the real-shell sweeps (macOS and Linux) on the same tag. Builds never run on ordinary pushes or pull requests: a tag is required, so check locally first (see CONTRIBUTING).
The manual steps below are what those workflows do, and the fallback.

1. Update `CHANGELOG.md` and the version in `Cargo.toml` (one workspace version).
2. Make sure the plugins copy is current (next section), then
   `cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
   and the sweeps: `python3 scripts/sweep/run.py zsh|bash|fish|nu`.
3. Build the assets for every target on a matching machine (or with `cross`):

   ```bash
   scripts/release.sh                    # this machine's target
   scripts/release.sh x86_64-unknown-linux-musl
   ```

   Each run adds `dist/shellcraft-<triple>.tar.gz` (one file inside: `shellcraft`) and its `.sha256`,
   and writes `dist/version.txt`. Linux assets use the musl target names (static binaries).
4. `scripts/release.sh formula` writes `dist/shellcraft.rb` from the tarballs in `dist/`; commit it to the
   tap repository (`cmdworks/homebrew-tap`, `Formula/shellcraft.rb`).
5. Tag and publish: `git tag vX.Y.Z && git push --tags`, then
   `gh release create vX.Y.Z dist/* --title vX.Y.Z`.

What users of these assets rely on, so do not rename them:

| Consumer | Uses |
| :--- | :--- |
| `install.sh` | `shellcraft-<triple>.tar.gz` and `.sha256` from `releases/latest/download` |
| `shellcraft update` | `version.txt`, then the tarball and checksum |
| the Homebrew formula | `releases/download/vX.Y.Z/shellcraft-<triple>.tar.gz` |

The checksum is published next to the download, so it catches corruption, not a compromised release.
Signing releases is planned.

## The official plugins copy

Plugins are edited only in [shellcraft-plugins](https://github.com/cmdworks/shellcraft-plugins).
Before a shellcraft release, tag that repository and vendor the tag:

```bash
git -C ../shellcraft-plugins tag vX.Y.Z
git -C ../shellcraft-plugins checkout vX.Y.Z
scripts/sync-plugins.sh ../shellcraft-plugins      # replaces vendored-plugins/, refuses a dirty checkout
cargo test -p shellcraft --test registry           # installs every official plugin and checks its hash
```

`vendored-plugins/SOURCE` records which tag was copied. Never edit `vendored-plugins/` by hand.
