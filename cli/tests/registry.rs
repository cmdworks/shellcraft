//! Registries, registry names, and the GitHub `owner/repo` path without GitHub: git rewrites
//! `https://github.com/` to a local directory, so the real URL code path runs against local repos.

use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

struct Env {
    root: PathBuf,
}

impl Env {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("sc-reg-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("cfg/shellcraft")).expect("mkdir");
        fs::create_dir_all(root.join("hub")).expect("mkdir");
        let e = Self { root };
        assert!(e.run(&["init"], "").0);
        e
    }

    fn hub(&self) -> PathBuf {
        self.root.join("hub")
    }

    fn run(&self, args: &[&str], stdin: &str) -> (bool, String) {
        self.run_with(args, stdin, false)
    }

    /// `online`: the official repository may be fetched (from the local hub, see below).
    fn run_with(&self, args: &[&str], stdin: &str, online: bool) -> (bool, String) {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_shellcraft"));
        if online {
            cmd.env_remove("SHELLCRAFT_OFFLINE");
        }
        let mut child = cmd
            .env("HOME", &self.root)
            .env("XDG_CONFIG_HOME", self.root.join("cfg"))
            .env("XDG_CACHE_HOME", self.root.join("cache"))
            .env("XDG_DATA_HOME", self.root.join("data"))
            .env("SHELL", "/bin/zsh")
            .env("SHELLCRAFT_NO_BG", "1")
            // every https://github.com/ URL is served from the local hub directory
            .env("GIT_CONFIG_COUNT", "1")
            .env(
                "GIT_CONFIG_KEY_0",
                format!("url.file://{}/.insteadOf", self.hub().display()),
            )
            .env("GIT_CONFIG_VALUE_0", "https://github.com/")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env_remove("SHELLCRAFT_CONFIG")
            .current_dir(&self.root)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn");
        child
            .stdin
            .take()
            .expect("stdin")
            .write_all(stdin.as_bytes())
            .expect("write");
        let out = child.wait_with_output().expect("wait");
        (
            out.status.success(),
            String::from_utf8_lossy(&out.stdout).into_owned()
                + &String::from_utf8_lossy(&out.stderr),
        )
    }

    fn lock(&self) -> String {
        fs::read_to_string(self.root.join("cfg/shellcraft/shellcraft.lock")).unwrap_or_default()
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
            "-C",
        ])
        .arg(dir)
        .args(args)
        .output()
        .expect("git");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn write_plugin(dir: &Path, name: &str, version: &str) {
    fs::create_dir_all(dir.join("bin")).expect("mkdir");
    fs::write(
        dir.join("shellcraft-plugin.toml"),
        format!("name = \"{name}\"\nversion = \"{version}\"\ndescription = \"{name} plugin\"\n\n[command.{name}]\nrun = \"bin/{name}\"\n"),
    )
    .expect("manifest");
    fs::write(
        dir.join(format!("bin/{name}")),
        format!("#!/bin/sh\necho {name}-{version}\n"),
    )
    .expect("script");
    fs::set_permissions(
        dir.join(format!("bin/{name}")),
        fs::Permissions::from_mode(0o755),
    )
    .expect("chmod");
}

/// A plugin repository at `hub/<owner>/<repo>.git`, tagged `v1`.
fn hub_plugin(e: &Env, owner: &str, repo: &str, name: &str) -> PathBuf {
    let dir = e.hub().join(owner).join(format!("{repo}.git"));
    write_plugin(&dir, name, "1.0.0");
    git(&dir, &["init", "-q", "."]);
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-q", "-m", "v1"]);
    git(&dir, &["tag", "v1"]);
    dir
}

#[test]
fn github_owner_repo_path_runs_against_a_local_hub() {
    let e = Env::new("github");
    let repo = hub_plugin(&e, "acme", "shellcraft-hello", "hello");
    let tagged = git(&repo, &["rev-parse", "v1"]);

    // `owner/repo@tag` builds https://github.com/acme/shellcraft-hello.git; git serves it locally
    let (ok, out) = e.run(&["plugin", "add", "acme/shellcraft-hello@v1", "--yes"], "");
    assert!(ok, "{out}");
    assert!(out.contains("source:   acme/shellcraft-hello@v1"), "{out}");
    let lock = e.lock();
    assert!(
        lock.contains(&tagged) && lock.contains("acme/shellcraft-hello@v1"),
        "{lock}"
    );
    assert!(e.run(&["hello"], "").1.contains("hello-1.0.0"));

    // a pinned plugin is not updated, even when the hub moved on
    write_plugin(&repo, "hello", "2.0.0");
    git(&repo, &["commit", "-q", "-am", "v2"]);
    assert!(e
        .run(&["plugin", "update", "--yes"], "")
        .1
        .contains("pinned"));
    assert!(e.run(&["plugin", "remove", "hello"], "").0);

    // unpinned: install the default branch, then update follows it
    let (ok, out) = e.run(&["plugin", "add", "acme/shellcraft-hello", "--yes"], "");
    assert!(
        ok && e.run(&["hello"], "").1.contains("hello-2.0.0"),
        "{out}"
    );
    write_plugin(&repo, "hello", "3.0.0");
    git(&repo, &["commit", "-q", "-am", "v3"]);
    let (ok, out) = e.run(&["plugin", "update", "--yes"], "");
    assert!(ok && out.contains("2.0.0 -> 3.0.0"), "{out}");
    assert!(e.run(&["hello"], "").1.contains("hello-3.0.0"));

    // a repository that does not exist fails cleanly
    let (ok, out) = e.run(&["plugin", "add", "acme/missing", "--yes"], "");
    assert!(!ok && out.contains("git clone"), "{out}");
}

#[test]
fn local_registry_search_info_add_by_name_and_hash_check() {
    let e = Env::new("registry");
    let reg = e.root.join("my-registry");
    // two plugins: a plain folder and a git repo on the hub
    let folder = e.root.join("src/todos");
    write_plugin(&folder, "todos", "0.5.0");
    hub_plugin(&e, "acme", "shellcraft-hello", "hello");

    assert!(
        e.run(&["registry", "init", reg.to_str().expect("utf8")], "")
            .0
    );
    assert!(
        !e.run(&["registry", "init", reg.to_str().expect("utf8")], "")
            .0,
        "not twice"
    );
    let (ok, out) = e.run(
        &[
            "registry",
            "publish",
            reg.to_str().expect("utf8"),
            folder.to_str().expect("utf8"),
        ],
        "",
    );
    assert!(ok && out.contains("published todos 0.5.0"), "{out}");
    let (ok, out) = e.run(
        &[
            "registry",
            "publish",
            reg.to_str().expect("utf8"),
            "acme/shellcraft-hello@v1",
        ],
        "",
    );
    assert!(ok && out.contains("published hello 1.0.0"), "{out}");
    let index = fs::read_to_string(reg.join("index.toml")).expect("index");
    assert!(
        index.contains("name = \"todos\"") && index.contains("hash = ") && index.contains("rev = "),
        "{index}"
    );

    // nothing configured yet
    let (ok, out) = e.run(&["plugin", "add", "todos", "--yes"], "");
    assert!(!ok && out.contains("no plugin named `todos`"), "{out}");

    assert!(
        e.run(&["registry", "add", reg.to_str().expect("utf8")], "")
            .0
    );
    assert!(
        !e.run(&["registry", "add", reg.to_str().expect("utf8")], "")
            .0,
        "duplicate"
    );
    let (_, list) = e.run(&["registry", "list"], "");
    assert!(list.contains("(2 plugins)"), "{list}");
    let (_, found) = e.run(&["plugin", "search", "plug"], "");
    assert!(
        found.contains("todos") && found.contains("hello"),
        "{found}"
    );
    assert!(e
        .run(&["plugin", "search", "zzz"], "")
        .1
        .contains("no plugin matches"));
    let (_, info) = e.run(&["plugin", "info", "hello"], "");
    assert!(
        info.contains("repo:     acme/shellcraft-hello@v1") && info.contains("hash:"),
        "{info}"
    );

    // add by name: from the folder entry and from the (GitHub-style) git entry
    let (ok, out) = e.run(&["plugin", "add", "todos", "--yes"], "");
    assert!(ok && out.contains("from registry"), "{out}");
    assert!(e.run(&["todos"], "").1.contains("todos-0.5.0"));
    let (ok, out) = e.run(&["plugin", "add", "hello", "--yes"], "");
    assert!(ok, "{out}");
    assert!(e.run(&["hello"], "").1.contains("hello-1.0.0"));
    assert!(!e.run(&["plugin", "add", "nonesuch", "--yes"], "").0);

    // the index pins a content hash: an entry with a wrong hash is refused, the others still work.
    // (entries are sorted by name, so the first `hash = "` belongs to `hello`)
    assert!(e.run(&["plugin", "remove", "todos"], "").0);
    let tampered = index.replacen("hash = \"", "hash = \"00", 1);
    fs::write(reg.join("index.toml"), tampered).expect("tamper");
    let (ok, out) = e.run(&["plugin", "add", "hello", "--yes"], "");
    assert!(
        !ok && out.contains("do not match the expected hash"),
        "{out}"
    );
    let (ok, out) = e.run(&["plugin", "add", "todos", "--yes"], "");
    assert!(ok, "{out}");

    assert!(
        e.run(&["registry", "remove", reg.to_str().expect("utf8")], "")
            .0
    );
    assert!(e
        .run(&["registry", "list"], "")
        .1
        .contains("no registries configured"));
}

/// Zero configuration: the official plugins are built into the binary. `install` finds them by
/// name (`base` pulls in the plugins it requires), says so when something is already installed,
/// `list` and `search` show them, `update` and `remove` work, and every pinned hash matches.
#[test]
fn install_update_remove_official_plugins_with_no_configuration() {
    let e = Env::new("official");
    let (ok, out) = e.run(&["search"], "");
    assert!(
        ok && out.contains("docker") && out.contains("themes"),
        "{out}"
    );
    let (ok, out) = e.run(&["install", "base"], "");
    assert!(
        ok && out.contains("needs git-aliases") && out.contains("restart your shell"),
        "{out}"
    );
    assert!(
        !out.contains("asks for"),
        "brief output, not the full summary: {out}"
    );
    let lock = e.lock();
    for n in ["base", "git-aliases", "docker", "themes", "cheats"] {
        assert!(
            lock.contains(&format!("name = \"{n}\"")),
            "{n} missing: {lock}"
        );
    }
    assert!(
        lock.contains("source = \"official:plugins/docker\""),
        "{lock}"
    );
    let (ok, out) = e.run(&["install", "docker"], "");
    assert!(ok && out.contains("already installed"), "{out}");
    let (ok, out) = e.run(&["list"], "");
    assert!(
        ok && out.contains("docker") && out.contains("installed"),
        "{out}"
    );
    let (ok, out) = e.run(&["update"], "");
    assert!(
        ok && out.contains("docker: up to date") && out.contains("5 plugins available"),
        "{out}"
    );
    // a theme and a cheat sheet from the plugins work
    let (ok, out) = e.run(&["theme", "nord"], "");
    assert!(ok, "installed theme can be chosen: {out}");
    let (_, cheat) = e.run(&["cheat", "git"], "");
    assert!(cheat.starts_with("git"), "{cheat}");
    let (ok, script) = e.run(&["apply", "--shell", "zsh", "--print"], "");
    assert!(
        ok && script.contains("alias gs='git status -sb'") && script.contains("dsh()"),
        "{script}"
    );
    let (ok, out) = e.run(&["remove", "docker"], "");
    assert!(ok && !e.lock().contains("official:plugins/docker"), "{out}");
    let (ok, out) = e.run(&["install", "nosuchthing"], "");
    assert!(!ok && out.contains("not a plugin"), "{out}");
}

/// The docker plugin's segment script: marker, context, compose, and nothing outside a project
/// (the `when` rule keeps the prompt from running it there; the script itself always prints).
#[test]
fn docker_plugin_segment_script() {
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../vendored-plugins/plugins/docker/bin/segment");
    let dir = std::env::temp_dir().join(format!("sc-docker-seg-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("mkdir");
    let run = |ctx: &str| {
        let out = Command::new(&script)
            .current_dir(&dir)
            .env("HOME", &dir)
            .env("DOCKER_CONTEXT", ctx)
            .env_remove("DOCKER_HOST")
            .output()
            .expect("run");
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    };
    fs::write(dir.join("Dockerfile"), "FROM scratch").expect("write");
    assert_eq!(run(""), "[tool] docker");
    assert_eq!(run("prod"), "[tool] docker:prod");
    assert_eq!(run("default"), "[tool] docker");
    fs::write(dir.join("compose.yaml"), "services: {}").expect("write");
    assert_eq!(run("prod"), "[tool] docker:prod +compose");
    let _ = fs::remove_dir_all(&dir);
}

/// Signed registries: with a trusted key configured, only a registry whose `index.toml.sig`
/// verifies is used; an edited index or a missing signature disables it.
#[test]
fn signed_registry_is_required_once_a_key_is_trusted() {
    let e = Env::new("signed");
    let reg = e.root.join("reg");
    let (reg_s, key_s) = (reg.to_str().expect("utf8"), e.root.join("secret.key"));
    let key = key_s.to_str().expect("utf8");
    let plugin = e.root.join("src/todos");
    write_plugin(&plugin, "todos", "0.5.0");
    assert!(e.run(&["registry", "init", reg_s], "").0);
    assert!(
        e.run(
            &["registry", "publish", reg_s, plugin.to_str().expect("utf8")],
            ""
        )
        .0
    );
    assert!(e.run(&["registry", "add", reg_s], "").0);
    let (ok, out) = e.run(&["registry", "keygen", key], "");
    assert!(ok, "{out}");
    let public = out
        .lines()
        .find_map(|l| l.strip_prefix("public key: "))
        .expect("public key")
        .to_string();

    // nothing trusted yet: an unsigned registry still works (development)
    assert!(e
        .run(&["plugin", "search", "todos"], "")
        .1
        .contains("todos"));

    // trust a key: the unsigned registry is now ignored
    assert!(e.run(&["registry", "trust", &public], "").0);
    assert!(
        !e.run(&["registry", "trust", "nothex"], "").0,
        "bad keys are refused"
    );
    let (_, out) = e.run(&["plugin", "search", "todos"], "");
    assert!(
        !out.contains("todos 0.5.0") && out.contains("not signed"),
        "{out}"
    );

    // signed: it works again, and installing by name goes through
    assert!(e.run(&["registry", "sign", reg_s, "--key", key], "").0);
    assert!(e
        .run(&["plugin", "search", "todos"], "")
        .1
        .contains("todos"));
    let (ok, out) = e.run(&["plugin", "add", "todos", "--yes"], "");
    assert!(ok, "{out}");

    // an edited index no longer matches its signature
    let index = reg.join("index.toml");
    fs::write(
        &index,
        fs::read_to_string(&index)
            .expect("read")
            .replace("0.5.0", "9.9.9"),
    )
    .expect("edit");
    let (_, out) = e.run(&["plugin", "search", "todos"], "");
    assert!(
        out.contains("does not match") && !out.contains("9.9.9"),
        "{out}"
    );
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("mkdir");
    for e in fs::read_dir(from).expect("read_dir").flatten() {
        let (src, dst) = (e.path(), to.join(e.file_name()));
        if src.is_dir() {
            copy_tree(&src, &dst);
        } else {
            fs::copy(&src, &dst).expect("copy");
        }
    }
}

/// The official plugins repository on the local hub: the vendored copy, with `docker` raised to
/// `version`, as `https://github.com/cmdworks/shellcraft-plugins.git` would serve it.
fn official_repo(e: &Env, version: &str) -> PathBuf {
    let repo = e.hub().join("cmdworks/shellcraft-plugins.git");
    let _ = fs::remove_dir_all(&repo);
    copy_tree(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../vendored-plugins"),
        &repo,
    );
    let _ = fs::remove_file(repo.join("SOURCE"));
    let manifest = repo.join("plugins/docker/shellcraft-plugin.toml");
    let text = fs::read_to_string(&manifest)
        .expect("manifest")
        .replace("version = \"0.1.0\"", &format!("version = \"{version}\""));
    fs::write(&manifest, text).expect("write");
    let (ok, out) = e.run(
        &[
            "registry",
            "publish",
            repo.to_str().expect("utf8"),
            repo.join("plugins/docker").to_str().expect("utf8"),
        ],
        "",
    );
    assert!(ok, "{out}");
    git(&repo, &["init", "-q", "."]);
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", version]);
    repo
}

/// `install` reads the official repository (cached clone, newer versions win over the embedded
/// copy), installs from it with a source that can be fetched again, and `update` follows it.
#[test]
fn official_repository_is_used_for_install_and_update() {
    let e = Env::new("official-remote");
    let repo = official_repo(&e, "9.9.9");

    // offline (the default under cargo): only the embedded copy
    let (_, out) = e.run(&["search", "docker"], "");
    assert!(out.contains("0.1.0") && !out.contains("9.9.9"), "{out}");

    // online: the repository's newer docker wins
    let (ok, out) = e.run_with(&["search", "docker"], "", true);
    assert!(ok && out.contains("9.9.9"), "{out}");
    let (ok, out) = e.run_with(&["install", "docker"], "", true);
    assert!(ok && out.contains("installing docker 9.9.9"), "{out}");
    let lock = e.lock();
    assert!(
        lock.contains(
            "source = \"https://github.com/cmdworks/shellcraft-plugins.git#plugins/docker\""
        ),
        "installed from the repository, not from a cache folder: {lock}"
    );
    // once cached it is also known offline
    let (_, out) = e.run(&["search", "docker"], "");
    assert!(out.contains("9.9.9"), "cached clone is read offline: {out}");

    // an embedded plugin the repository does not have newer still installs from the binary
    let (ok, out) = e.run_with(&["install", "cheats"], "", true);
    assert!(ok, "{out}");
    assert!(e.lock().contains("official:plugins/cheats"), "{}", e.lock());

    // the repository publishes 9.9.10: update fetches it
    let text =
        fs::read_to_string(repo.join("plugins/docker/shellcraft-plugin.toml")).expect("manifest");
    fs::write(
        repo.join("plugins/docker/shellcraft-plugin.toml"),
        text.replace("9.9.9", "9.9.10"),
    )
    .expect("write");
    let (ok, out) = e.run(
        &[
            "registry",
            "publish",
            repo.to_str().expect("utf8"),
            repo.join("plugins/docker").to_str().expect("utf8"),
        ],
        "",
    );
    assert!(ok, "{out}");
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "9.9.10"]);
    let (ok, out) = e.run_with(&["update"], "", true);
    assert!(ok && out.contains("9.9.9 -> 9.9.10"), "{out}");
    assert!(e.lock().contains("version = \"9.9.10\""));
    let (_, again) = e.run_with(&["update"], "", true);
    assert!(again.contains("docker: up to date"), "{again}");
}

/// Sources: a folder inside a repository (`owner/repo/folder`, `url#folder`), pinned or not.
#[test]
fn plugins_install_from_a_folder_of_a_git_repository() {
    let e = Env::new("subfolder");
    let repo = official_repo(&e, "1.0.0");
    // owner/repo/folder
    let (ok, out) = e.run(
        &[
            "plugin",
            "add",
            "cmdworks/shellcraft-plugins/plugins/git-aliases",
            "--yes",
        ],
        "",
    );
    assert!(ok, "{out}");
    assert!(e
        .lock()
        .contains("source = \"cmdworks/shellcraft-plugins/plugins/git-aliases\""));
    let (_, out) = e.run(&["plugin", "update", "git-aliases", "--yes"], "");
    assert!(out.contains("git-aliases: up to date"), "{out}");
    // url#folder, pinned to a tag: never updated
    git(&repo, &["tag", "v1"]);
    let url = format!("file://{}#plugins/cheats", repo.display());
    let (ok, out) = e.run(&["plugin", "add", &url, "--yes"], "");
    assert!(ok, "{out}");
    let (ok, out) = e.run(
        &[
            "plugin",
            "add",
            "cmdworks/shellcraft-plugins/plugins/themes@v1",
            "--yes",
        ],
        "",
    );
    assert!(ok, "{out}");
    let (_, out) = e.run(&["plugin", "update", "themes", "--yes"], "");
    assert!(out.contains("themes: pinned"), "{out}");
    // a folder that is not there, and an unsafe one
    let (ok, out) = e.run(
        &[
            "plugin",
            "add",
            "cmdworks/shellcraft-plugins/plugins/nothing-here",
            "--yes",
        ],
        "",
    );
    assert!(!ok && out.contains("no folder"), "{out}");
    let (ok, out) = e.run(
        &["plugin", "add", "cmdworks/shellcraft-plugins/../x", "--yes"],
        "",
    );
    assert!(!ok, "{out}");
}

/// A registry in a folder of a repository (`owner/repo/community`): the community list. Its entries
/// point at other repositories; installs record where they came from.
#[test]
fn a_registry_can_live_in_a_folder_of_a_repository() {
    let e = Env::new("folder-registry");
    let repo = official_repo(&e, "1.0.0");
    // a community list inside the same repository, listing one plugin that is a folder of the repo
    fs::create_dir_all(repo.join("community")).expect("mkdir");
    let (ok, out) = e.run(
        &[
            "registry",
            "init",
            repo.join("community").to_str().expect("utf8"),
        ],
        "",
    );
    assert!(ok, "{out}");
    fs::remove_dir_all(repo.join("community/.git")).ok();
    let (ok, out) = e.run(
        &[
            "registry",
            "publish",
            repo.join("community").to_str().expect("utf8"),
            "cmdworks/shellcraft-plugins/plugins/cheats",
        ],
        "",
    );
    assert!(ok, "{out}");
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "community"]);
    let (ok, out) = e.run(
        &["registry", "add", "cmdworks/shellcraft-plugins/community"],
        "",
    );
    assert!(ok, "{out}");
    let (ok, out) = e.run_with(&["registry", "list"], "", true);
    assert!(
        ok && out.contains("cmdworks/shellcraft-plugins/community  (1 plugins)"),
        "{out}"
    );
}
