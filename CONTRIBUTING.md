# Contributing to shellcraft

Thank you for helping. This repository is the program: the binary, the plugin API, the documentation
of both. Plugins (official and community) live in
[cmdworks/shellcraft-plugins](https://github.com/cmdworks/shellcraft-plugins).

## Where to start

- **A bug**: open an issue with your OS, shell and version (`shellcraft --version`, `echo $SHELL`), what you
  did, what you expected and `shellcraft doctor` output.
- **An idea**: open an issue first for anything larger than a fix, so we agree on the shape before you write it.
- **Good first changes**: docs, tests, a shell snippet that is wrong in one shell, a missing config
  option in `docs/configuration.md`.
- **A plugin**: not here; see the plugins repository.

## Setup

```bash
git clone https://github.com/cmdworks/shellcraft && cd shellcraft
cargo build                      # needs a stable Rust toolchain (rust-toolchain.toml)
scripts/try.sh zsh               # a throw-away shell with your build; your own rc files are untouched
                                 # (or: direnv allow, then try-zsh / try-bash / try-fish / try-nu and `sc`)
```

Layout and where new code goes: [docs/architecture.md](docs/architecture.md).

## Before you open a pull request

CI (`.github/workflows/ci.yml`) runs the same checks on macOS and Linux, but only when a version tag is pushed, so run them locally first:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
python3 scripts/sweep/run.py zsh     # drives a real shell in a pty; also bash, fish, nu (and pwsh)
```

Touching the shell templates? Regenerate snapshots with `INSTA_UPDATE=always cargo test -p shellcraft-shell`
and read the diff: the snapshots are the exact scripts users run.
Touching performance-relevant code? `shellcraft bench` and
`python3 scripts/bench/compare-omz.py` (see [docs/performance.md](docs/performance.md)); say what you measured.

## Rules

1. **Small and focused.** One change per pull request, no drive-by refactors.
2. **Hot paths stay fast.** `prompt`, `jump`, `hist`, `hook`, `complete`: no async runtime, no network, no
   config parsing; budgets are in [docs/architecture.md](docs/architecture.md).
3. **No `unwrap()` outside tests**; `anyhow` in binaries, `thiserror` in libraries; `forbid(unsafe_code)` stays.
4. **Dependencies need a reason.** State what the dependency is for and its license in the pull request
   (and in the list in `docs/architecture.md`); prefer the standard library.
5. **Generated shell code must parse**: `zsh -n`, `bash -n`, `fish -n`, `nu --commands`, or `pwsh -NoProfile -Command`.
   Tests that touch real shells skip when the shell is not installed.
6. **Tests never use the network or run downloaded code**; use local fixtures (loopback servers, `file://`
   with the dev artifacts variable, git URL rewriting as in `cli/tests/registry.rs`).
7. **Public behaviour changes update the docs** (`docs/commands.md`, `docs/configuration.md`, `docs/plugin-api.md`) and `CHANGELOG.md`.
8. **Plugin API changes are additive.** A breaking change raises the API version and is discussed in an
   issue first ([docs/plugin-api.md](docs/plugin-api.md#stability-and-versioning)).

## Commits and reviews

Short imperative subject lines ("fish: draw the prompt in the shell"), a body when the why is not obvious.
Reviewers aim to respond within a week. Squash noise before review.

## Licensing of contributions

By contributing you agree that your contribution is licensed under the MIT license of this repository
(inbound = outbound). Do not submit code you do not have the right to relicense.

## Conduct

Be kind and specific. Disagree with the code, not the person. Harassment of any kind is not tolerated
and leads to removal from the project. Report conduct problems privately to the maintainers (see
[SECURITY.md](SECURITY.md) for how to reach them).
