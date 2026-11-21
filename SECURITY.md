# Security

## Reporting a vulnerability

Please do not open a public issue. Use GitHub's private vulnerability reporting on this repository
(Security tab, "Report a vulnerability"). You will get an answer within a week.

## Supported versions

The latest release. shellcraft is early development software; fixes go into the next release.

## What is in scope

- The install script and `shellcraft update` (what they download, verify and replace).
- Handling of plugin sources, hashes, registries and signatures (a changed plugin loading as the
  reviewed one; a registry accepted without the signature a user required).
- The generated init scripts (quoting, injection through config values, paths with unusual characters).
- Reading untrusted input: manifests, `index.toml`, `--help` output used for completion, history.

## Trust model, plainly

- **Plugins run as your user and are not sandboxed.** A plugin can do anything you can. Read plugins before
  installing them. shellcraft pins each installed plugin to a content hash in `shellcraft.lock` and skips it
  if its files change; `[permissions]` in a manifest is what the author says, not something enforced.
- **Registries** can be signed (Ed25519); once you run `shellcraft registry trust <key>`, unsigned or
  mismatching registries are ignored. Without a trusted key, registries are not verified.
- **Releases** are checked against a SHA-256 published next to the download over HTTPS. That catches
  corruption and a swapped file, not a compromised release. Signed releases are planned.
- **Completion** runs `<command> --help` for commands without completion of their own, never for programs in
  the current directory or world-writable folders, never for shells, interpreters, editors or `rm`.
  `[completion] help = false` turns it off.

## Disclosure

We aim to fix and release within 30 days of a confirmed report and credit reporters who want credit.
