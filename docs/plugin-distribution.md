# How plugins are found, installed and built

This page is the rules. Guides for authors are in
[shellcraft-plugins](https://github.com/cmdworks/shellcraft-plugins/tree/main/docs).

## What is a plugin

A folder with a `shellcraft-plugin.toml` (see [plugin-api.md](plugin-api.md)). In a git repository the
manifest is at the root, or in a folder you name. The plugin's **name and version come from the
manifest**, never from the repository name. Git tags are the release versions.

## Where a plugin can come from

| You type | Meaning |
| :--- | :--- |
| `shellcraft install docker` | an official plugin by name (see below) |
| `shellcraft install owner/repo` | GitHub repository `owner/repo`, default branch |
| `shellcraft install owner/repo@v1.2.0` | the same, pinned to a tag, branch or commit |
| `shellcraft install owner/repo/path/to/plugin[@rev]` | a plugin in a folder of a repository |
| `shellcraft install https://gitlab.com/me/plugin.git` | any git URL (`https://`, `ssh://`, `file://`, `git@host:path`) |
| `shellcraft install <url>#path/to/plugin` | a folder of any git repository |
| `shellcraft install ./my-plugin` | a local folder (copied) |
| `shellcraft plugin link ./my-plugin` | a local folder used live, for development |
| `shellcraft install name` after `shellcraft registry add <source>` | a name from your own registry |

All of these work together: you can mix official plugins, your own repositories and registries.

**Pinned or not.** `owner/repo` and plain URLs follow the repository: `shellcraft update` fetches the newer
commit, shows what changed and applies it. A source with `@rev` is pinned and never updated. Every
install records the exact commit and a SHA-256 of the files in `shellcraft.lock`; a plugin whose files no
longer match is skipped with a warning.

## Official plugins

They are developed in [cmdworks/shellcraft-plugins](https://github.com/cmdworks/shellcraft-plugins). Two
copies are consulted, newest version wins:

1. the copy **embedded in the shellcraft binary** (works offline, always present);
2. a shallow clone of the repository in your cache, refreshed at most once a day when you run `install`,
   `search` or `update`, and immediately by `update`. Without network it is skipped silently.

A plugin installed from the repository is recorded as `<repo url>#plugins/<name>` pinned to the commit
that was read, so another machine (`shellcraft pull`) and `update` fetch it the same way.
`SHELLCRAFT_OFFLINE=1` stops all network use (registry refresh and the shellcraft update check).

## What shellcraft will and will not do to install a plugin

- It **copies files** and records a hash. It runs nothing from the plugin while installing.
- It **never builds code** on your machine and never runs an install script, a build command or
  anything downloaded, except the `[[binary]]` downloads below.
- Plugins run as you afterwards; there is no sandbox. Read what you install.

## Plugins that need compiled code: prebuilt binaries

A plugin written in Rust, Go or anything compiled ships **prebuilt executables per platform**,
built by the author's CI and published as release assets (any `https` URL works; GitHub releases are the
usual place):

```toml
[[binary]]
name = "todo"
target = "aarch64-apple-darwin"
url = "https://github.com/me/todo/releases/download/v1.0.0/todo-aarch64-apple-darwin.tar.gz"
sha256 = "...64 hex digits..."
archive = "tar.gz"
member = "todo"
```

One entry per platform. The targets shellcraft knows are `aarch64-apple-darwin`, `x86_64-apple-darwin`,
`x86_64-unknown-linux-musl` and `aarch64-unknown-linux-musl`. At install the entry for your platform is
downloaded, its SHA-256 is checked, the file is unpacked into the data folder and its checksum recorded; on
every `apply` it is checked again, and a missing or changed file disables the plugin with a warning. A
plugin with no entry for your platform is refused with a message saying which platforms it supports.

Why prebuilt and not "build on install": building needs a toolchain the user may not have, takes minutes,
runs arbitrary build scripts, and gives different results on different machines. A checksummed artifact
built in public CI is reproducible, fast and inspectable.

The author's workflow has two tags, because a tag is a snapshot: a build tag makes CI build the four targets
and upload the archives; CI prints the `[[binary]]` blocks; the author pastes them into the manifest,
commits, and tags the release that users install. A ready workflow and the steps are in
`templates/github-actions/plugin-release.yml` and `docs/binary-plugins.md` in shellcraft-plugins.

## Names and registries

`shellcraft install <name>` looks the name up, in this order: official plugins, then the registries you
added (`shellcraft registry add`), in order. A registry is a git repository or folder with an
`index.toml`; entries may point at other repositories, with a pinned `rev` and `hash`. Registries can be
signed (`shellcraft registry trust`). The community list in shellcraft-plugins is such a registry: `shellcraft registry add cmdworks/shellcraft-plugins/community`.
