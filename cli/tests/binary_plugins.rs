//! Plugins with prebuilt binaries. Artifacts come from a local directory
//! (`SHELLCRAFT_DEV_ARTIFACTS`), so no network is used.

use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use sha2::{Digest, Sha256};

struct Env {
    root: PathBuf,
    artifacts: PathBuf,
}

impl Env {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("sc-bin-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("cfg/shellcraft")).expect("mkdir");
        let artifacts = root.join("artifacts");
        fs::create_dir_all(&artifacts).expect("mkdir");
        let e = Self { root, artifacts };
        assert!(e.run(&["init"], true).0);
        e
    }

    fn run(&self, args: &[&str], dev: bool) -> (bool, String) {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_shellcraft"));
        cmd.env("HOME", &self.root)
            .env("XDG_CONFIG_HOME", self.root.join("cfg"))
            .env("XDG_CACHE_HOME", self.root.join("cache"))
            .env("XDG_DATA_HOME", self.root.join("data"))
            .env("SHELL", "/bin/zsh")
            .env("SHELLCRAFT_NO_BG", "1")
            .env("SHELLCRAFT_DEV_TARGET", "test-target")
            .env_remove("SHELLCRAFT_CONFIG")
            .current_dir(&self.root)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if dev {
            cmd.env("SHELLCRAFT_DEV_ARTIFACTS", &self.artifacts);
        } else {
            cmd.env_remove("SHELLCRAFT_DEV_ARTIFACTS");
        }
        let mut child = cmd.spawn().expect("spawn");
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

    fn lock(&self) -> String {
        fs::read_to_string(self.root.join("cfg/shellcraft/shellcraft.lock")).unwrap_or_default()
    }

    fn bin(&self, plugin: &str) -> PathBuf {
        self.root.join("data/shellcraft/plugin-bin").join(plugin)
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn sha(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// `tool.tar.gz` holding `bin/tool` (a script), made with the system `tar`.
fn make_tarball(dir: &Path, script: &str) -> Vec<u8> {
    let stage = dir.join("stage");
    fs::create_dir_all(stage.join("bin")).expect("mkdir");
    fs::write(stage.join("bin/tool"), script).expect("script");
    fs::set_permissions(stage.join("bin/tool"), fs::Permissions::from_mode(0o755)).expect("chmod");
    let ok = Command::new("tar")
        .args(["czf"])
        .arg(dir.join("tool.tar.gz"))
        .arg("-C")
        .arg(&stage)
        .arg("bin")
        .status()
        .expect("tar")
        .success();
    assert!(ok, "tar failed");
    fs::read(dir.join("tool.tar.gz")).expect("tarball")
}

fn plugin_dir(e: &Env, binaries: &str) -> PathBuf {
    let src = e.root.join("src/binplug");
    fs::create_dir_all(&src).expect("mkdir");
    fs::write(
        src.join("shellcraft-plugin.toml"),
        format!("name = \"binplug\"\nversion = \"1.0.0\"\n{binaries}\n[command.tool]\nrun = \"@tool\"\nabout = \"a prebuilt tool\"\n"),
    )
    .expect("manifest");
    src
}

#[test]
fn archive_binary_installs_verifies_runs_and_is_removed() {
    let e = Env::new("archive");
    let tarball = make_tarball(&e.artifacts, "#!/bin/sh\necho prebuilt-says-$1\n");
    let src = plugin_dir(&e, &format!("[[binary]]\nname = \"tool\"\ntarget = \"test-target\"\nurl = \"tool.tar.gz\"\nsha256 = \"{}\"\narchive = \"tar.gz\"\nmember = \"bin/tool\"\n", sha(&tarball)));
    let s = src.to_str().expect("utf8");

    // without the development directory only https downloads are allowed
    let (ok, out) = e.run(&["plugin", "add", s, "--yes"], false);
    assert!(!ok && out.contains("SHELLCRAFT_DEV_ARTIFACTS"), "{out}");
    assert!(
        !e.root.join("data/shellcraft/plugins/binplug").exists(),
        "nothing is left behind"
    );

    let (ok, out) = e.run(&["plugin", "add", s, "--yes"], true);
    assert!(ok, "{out}");
    assert!(
        out.contains("binary:   tool from tool.tar.gz"),
        "the summary names the download: {out}"
    );
    assert!(e.bin("binplug").join("tool").is_file());
    assert!(
        e.lock().contains("[plugin.binaries]") || e.lock().contains("binaries"),
        "{}",
        e.lock()
    );
    let (ok, out) = e.run(&["tool", "hello"], false);
    assert!(!ok || out.contains("prebuilt-says-hello"), "{out}");
    assert!(out.contains("prebuilt-says-hello"), "{out}");

    // the downloaded file is covered by the lock: change it and the plugin is skipped
    fs::write(e.bin("binplug").join("tool"), "#!/bin/sh\necho evil\n").expect("tamper");
    let (_, out) = e.run(&["apply", "--shell", "zsh"], false);
    assert!(out.contains("binary `tool` is missing or changed"), "{out}");
    assert!(
        !e.run(&["tool"], false).1.contains("evil"),
        "a tampered binary is not run"
    );

    // removing the plugin removes its binaries
    assert!(e.run(&["plugin", "remove", "binplug"], false).0);
    assert!(!e.bin("binplug").exists());
}

#[test]
fn wrong_checksum_and_missing_platform_refuse_the_install() {
    let e = Env::new("refuse");
    let tarball = make_tarball(&e.artifacts, "#!/bin/sh\necho x\n");

    let bad = plugin_dir(&e, &format!("[[binary]]\nname = \"tool\"\ntarget = \"test-target\"\nurl = \"tool.tar.gz\"\nsha256 = \"{}\"\narchive = \"tar.gz\"\nmember = \"bin/tool\"\n", "0".repeat(64)));
    let (ok, out) = e.run(
        &["plugin", "add", bad.to_str().expect("utf8"), "--yes"],
        true,
    );
    assert!(!ok && out.contains("checksum mismatch"), "{out}");
    assert!(!e.root.join("data/shellcraft/plugins/binplug").exists() && !e.bin("binplug").exists());

    let other = plugin_dir(&e, &format!("[[binary]]\nname = \"tool\"\ntarget = \"some-other-target\"\nurl = \"tool.tar.gz\"\nsha256 = \"{}\"\narchive = \"tar.gz\"\nmember = \"bin/tool\"\n", sha(&tarball)));
    let (ok, out) = e.run(
        &["plugin", "add", other.to_str().expect("utf8"), "--yes"],
        true,
    );
    assert!(
        !ok && out.contains("no prebuilt binary of `tool` for test-target"),
        "{out}"
    );

    // a path that escapes the artifacts directory is refused even in development mode
    fs::write(e.root.join("secret.txt"), "s").expect("secret");
    let escape = plugin_dir(&e, &format!("[[binary]]\nname = \"tool\"\ntarget = \"test-target\"\nurl = \"file://{}\"\nsha256 = \"{}\"\n", e.root.join("secret.txt").display(), sha(b"s")));
    let (ok, out) = e.run(
        &["plugin", "add", escape.to_str().expect("utf8"), "--yes"],
        true,
    );
    assert!(
        !ok && out.contains("outside the development artifacts directory"),
        "{out}"
    );
}

#[test]
fn raw_binary_via_link_and_update_refetches() {
    let e = Env::new("raw");
    let v1 = b"#!/bin/sh\necho raw-v1\n";
    fs::write(e.artifacts.join("tool-v1"), v1).expect("artifact");
    let src = plugin_dir(&e, &format!("[[binary]]\nname = \"tool\"\ntarget = \"test-target\"\nurl = \"tool-v1\"\nsha256 = \"{}\"\n", sha(v1)));
    assert!(
        e.run(&["plugin", "link", src.to_str().expect("utf8")], true)
            .0
    );
    assert!(e.run(&["tool"], false).1.contains("raw-v1"));
    assert!(e.run(&["plugin", "remove", "binplug"], false).0);

    assert!(
        e.run(
            &["plugin", "add", src.to_str().expect("utf8"), "--yes"],
            true
        )
        .0
    );
    // the author ships v2 (new artifact and manifest); update downloads it
    let v2 = b"#!/bin/sh\necho raw-v2\n";
    fs::write(e.artifacts.join("tool-v2"), v2).expect("artifact");
    let manifest = fs::read_to_string(src.join("shellcraft-plugin.toml")).expect("manifest");
    fs::write(
        src.join("shellcraft-plugin.toml"),
        manifest
            .replace("tool-v1", "tool-v2")
            .replace(&sha(v1), &sha(v2))
            .replace("version = \"1.0.0\"", "version = \"2.0.0\""),
    )
    .expect("manifest");
    let (ok, out) = e.run(&["plugin", "update", "--yes"], true);
    assert!(ok, "{out}");
    assert!(e.run(&["tool"], false).1.contains("raw-v2"));
}
