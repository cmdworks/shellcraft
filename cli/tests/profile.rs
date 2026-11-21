//! Profile repositories: `pull` on a "new machine" installs the pinned plugins, `push` sends
//! changes back. Everything runs against local repositories; GitHub URLs are rewritten to a
//! local hub directory by git itself.

use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

struct Machine {
    root: PathBuf,
    hub: PathBuf,
}

impl Machine {
    fn new(name: &str, hub: &Path) -> Self {
        let root = std::env::temp_dir().join(format!("sc-profile-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("cfg/shellcraft")).expect("mkdir");
        let m = Self {
            root,
            hub: hub.to_path_buf(),
        };
        assert!(m.run(&["init"]).0);
        m
    }

    fn run(&self, args: &[&str]) -> (bool, String) {
        let mut child = Command::new(env!("CARGO_BIN_EXE_shellcraft"))
            .env("HOME", &self.root)
            .env("XDG_CONFIG_HOME", self.root.join("cfg"))
            .env("XDG_CACHE_HOME", self.root.join("cache"))
            .env("XDG_DATA_HOME", self.root.join("data"))
            .env("SHELL", "/bin/zsh")
            .env("SHELLCRAFT_NO_BG", "1")
            .env("GIT_CONFIG_COUNT", "1")
            .env(
                "GIT_CONFIG_KEY_0",
                format!("url.file://{}/.insteadOf", self.hub.display()),
            )
            .env("GIT_CONFIG_VALUE_0", "https://github.com/")
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@t")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@t")
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
            .write_all(b"")
            .expect("write");
        let out = child.wait_with_output().expect("wait");
        (
            out.status.success(),
            String::from_utf8_lossy(&out.stdout).into_owned()
                + &String::from_utf8_lossy(&out.stderr),
        )
    }

    fn config(&self) -> PathBuf {
        self.root.join("cfg/shellcraft/shellcraft.toml")
    }

    fn lock(&self) -> String {
        fs::read_to_string(self.root.join("cfg/shellcraft/shellcraft.lock")).unwrap_or_default()
    }
}

impl Drop for Machine {
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

fn hub_plugin(hub: &Path, owner: &str, name: &str, version: &str, extra: &str) {
    let dir = hub.join(owner).join(format!("{name}.git"));
    fs::create_dir_all(dir.join("bin")).expect("mkdir");
    fs::write(
        dir.join("shellcraft-plugin.toml"),
        format!("name = \"{name}\"\nversion = \"{version}\"\n{extra}\n[command.{name}]\nrun = \"bin/{name}\"\n"),
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
    git(&dir, &["init", "-q", "."]);
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-q", "-m", version]);
    git(&dir, &["tag", "v1"]);
}

#[test]
fn pull_installs_the_pinned_plugins_and_push_sends_changes_back() {
    let hub = std::env::temp_dir().join(format!("sc-profile-hub-{}", std::process::id()));
    let _ = fs::remove_dir_all(&hub);
    hub_plugin(&hub, "acme", "alpha", "1.0.0", "");
    hub_plugin(&hub, "acme", "beta", "1.0.0", "requires = [\"alpha\"]\n");

    // machine one: the setup that becomes the profile
    let one = Machine::new("one", &hub);
    fs::write(one.config(), "[theme]\nname = \"nord\"\n").expect("config");
    // install beta first on purpose: its requirement must be met, so alpha goes first
    assert!(one.run(&["plugin", "add", "acme/alpha@v1", "--yes"]).0);
    assert!(one.run(&["plugin", "add", "acme/beta@v1", "--yes"]).0);

    // publish it as a profile: a git working copy pushed to a bare "remote"
    let src = hub.join("profile-src");
    fs::create_dir_all(&src).expect("mkdir");
    fs::copy(one.config(), src.join("shellcraft.toml")).expect("copy");
    fs::write(src.join("shellcraft.lock"), one.lock()).expect("lock");
    git(&src, &["init", "-q", "-b", "main", "."]);
    git(&src, &["add", "."]);
    git(&src, &["commit", "-q", "-m", "profile"]);
    let remote = hub.join("profile-remote.git");
    git(
        &hub,
        &[
            "init",
            "-q",
            "--bare",
            "-b",
            "main",
            remote.to_str().expect("utf8"),
        ],
    );
    git(
        &src,
        &["remote", "add", "origin", remote.to_str().expect("utf8")],
    );
    git(&src, &["push", "-q", "origin", "main"]);

    // machine two is new: one command brings the config and the plugins
    let two = Machine::new("two", &hub);
    let (ok, out) = two.run(&["pull", remote.to_str().expect("utf8"), "--yes"]);
    assert!(ok, "{out}");
    assert!(
        out.contains("plugin:  alpha 1.0.0") && out.contains("plugin:  beta 1.0.0"),
        "{out}"
    );
    assert!(fs::read_to_string(two.config())
        .expect("config")
        .contains("nord"));
    assert!(
        two.run(&["alpha"]).1.contains("alpha-1.0.0")
            && two.run(&["beta"]).1.contains("beta-1.0.0")
    );
    let (a, b) = (one.lock(), two.lock());
    for name in ["alpha", "beta"] {
        let rev = |l: &str| {
            let at = l.find(&format!("name = \"{name}\"")).expect("entry");
            l[at..]
                .lines()
                .find(|x| x.starts_with("rev = "))
                .expect("rev")
                .to_string()
        };
        assert_eq!(rev(&a), rev(&b), "same commit on both machines for {name}");
    }

    // machine two changes its config and pushes; the remote gets the commit
    fs::write(two.config(), "[theme]\nname = \"dracula\"\n").expect("config");
    let (ok, out) = two.run(&["push", "-m", "switch to dracula"]);
    assert!(ok && out.contains("pushed to origin"), "{out}");
    assert_eq!(
        git(&remote, &["log", "-1", "--format=%s", "main"]),
        "switch to dracula"
    );
    assert!(git(&remote, &["show", "main:shellcraft.toml"]).contains("dracula"));
    assert!(
        two.run(&["push"]).1.contains("up to date"),
        "pushing twice does nothing"
    );

    // machine one pulls the update; its existing plugins are kept (hash unchanged), config replaced
    let (ok, out) = one.run(&["pull", remote.to_str().expect("utf8"), "--yes"]);
    assert!(ok, "{out}");
    assert!(fs::read_to_string(one.config())
        .expect("config")
        .contains("dracula"));
    assert!(one.run(&["alpha"]).1.contains("alpha-1.0.0"));
    let backups = fs::read_dir(one.root.join("cfg/shellcraft"))
        .expect("dir")
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().contains("pre-profile"))
        .count();
    assert_eq!(backups, 1, "the previous config was backed up");
    let _ = fs::remove_dir_all(&hub);
}

#[test]
fn pull_refuses_bad_profiles_and_wrong_hashes() {
    let hub = std::env::temp_dir().join(format!("sc-profile-hub2-{}", std::process::id()));
    let _ = fs::remove_dir_all(&hub);
    hub_plugin(&hub, "acme", "alpha", "1.0.0", "");
    let m = Machine::new("bad", &hub);

    let empty = hub.join("empty-profile");
    fs::create_dir_all(&empty).expect("mkdir");
    let (ok, out) = m.run(&["pull", empty.to_str().expect("utf8"), "--yes"]);
    assert!(!ok && out.contains("has no shellcraft.toml"), "{out}");

    let broken = hub.join("broken-profile");
    fs::create_dir_all(&broken).expect("mkdir");
    fs::write(broken.join("shellcraft.toml"), "[nonsense]\n").expect("config");
    assert!(!m.run(&["pull", broken.to_str().expect("utf8"), "--yes"]).0);

    // a lock entry whose hash does not match what the repository serves is refused
    let prof = hub.join("tampered-profile");
    fs::create_dir_all(&prof).expect("mkdir");
    fs::write(prof.join("shellcraft.toml"), "[theme]\nname = \"nord\"\n").expect("config");
    let rev = git(&hub.join("acme/alpha.git"), &["rev-parse", "HEAD"]);
    fs::write(
        prof.join("shellcraft.lock"),
        format!("[[plugin]]\nname = \"alpha\"\nversion = \"1.0.0\"\nsource = \"acme/alpha\"\nrev = \"{rev}\"\nhash = \"{}\"\napproved = []\ndev = false\n", "0".repeat(64)),
    )
    .expect("lock");
    let (ok, out) = m.run(&["pull", prof.to_str().expect("utf8"), "--yes"]);
    assert!(
        !ok && out.contains("do not match the expected hash"),
        "{out}"
    );
    assert!(
        m.run(&["alpha"]).1.contains("unknown command"),
        "nothing was installed"
    );

    // push without a pulled git profile says what to do
    let fresh = Machine::new("nopush", &hub);
    let (ok, out) = fresh.run(&["push"]);
    assert!(!ok && out.contains("run `shellcraft pull"), "{out}");
    let _ = fs::remove_dir_all(&hub);
}
