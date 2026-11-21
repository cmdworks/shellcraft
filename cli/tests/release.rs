//! The install script and `shellcraft update` against local fixtures (no network): a release is
//! the running test binary packaged the way `scripts/release.sh` packages it.

use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;

fn triple() -> &'static str {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "aarch64-apple-darwin",
        ("macos", _) => "x86_64-apple-darwin",
        (_, "aarch64") => "aarch64-unknown-linux-musl",
        _ => "x86_64-unknown-linux-musl",
    }
}

fn sha(path: &Path) -> String {
    let out = Command::new("sh")
        .args([
            "-c",
            &format!(
                "(sha256sum '{p}' 2>/dev/null || shasum -a 256 '{p}') | cut -d' ' -f1",
                p = path.display()
            ),
        ])
        .output()
        .expect("sha");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// A release folder: `shellcraft-<triple>.tar.gz`, its `.sha256`, `version.txt`.
fn release(root: &Path, version: &str, bad_sum: bool, payload: Option<&str>) -> PathBuf {
    let dist = root.join("dist");
    fs::create_dir_all(&dist).expect("mkdir");
    let stage = root.join("stage");
    fs::create_dir_all(&stage).expect("mkdir");
    match payload {
        Some(script) => {
            use std::os::unix::fs::PermissionsExt;
            fs::write(stage.join("shellcraft"), script).expect("payload");
            fs::set_permissions(stage.join("shellcraft"), fs::Permissions::from_mode(0o755))
                .expect("chmod");
        }
        None => {
            fs::copy(env!("CARGO_BIN_EXE_shellcraft"), stage.join("shellcraft")).expect("copy");
        }
    }
    let tarball = dist.join(format!("shellcraft-{}.tar.gz", triple()));
    assert!(Command::new("tar")
        .arg("-C")
        .arg(&stage)
        .arg("-czf")
        .arg(&tarball)
        .arg("shellcraft")
        .status()
        .expect("tar")
        .success());
    let sum = if bad_sum {
        "0".repeat(64)
    } else {
        sha(&tarball)
    };
    fs::write(format!("{}.sha256", tarball.display()), format!("{sum}\n")).expect("sum");
    fs::write(dist.join("version.txt"), format!("{version}\n")).expect("version");
    dist
}

fn tools_present() -> bool {
    ["tar", "curl"]
        .iter()
        .all(|t| Command::new(t).arg("--version").output().is_ok())
}

#[test]
fn install_script_installs_wires_the_shell_and_refuses_a_bad_checksum() {
    if !tools_present() {
        return;
    }
    let root = std::env::temp_dir().join(format!("sc-install-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("home")).expect("mkdir");
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../install.sh");
    let run = |dist: &Path, bindir: &Path| {
        Command::new("sh")
            .arg(&script)
            .env("HOME", root.join("home"))
            .env("XDG_CONFIG_HOME", root.join("home/cfg"))
            .env("XDG_CACHE_HOME", root.join("home/cache"))
            .env("XDG_DATA_HOME", root.join("home/data"))
            .env("SHELL", "/bin/zsh")
            .env("SHELLCRAFT_BASE_URL", format!("file://{}", dist.display()))
            .env("SHELLCRAFT_INSTALL_DIR", bindir)
            .output()
            .expect("run install.sh")
    };

    let bad = release(&root.join("bad"), "9.9.9", true, None);
    let out = run(&bad, &root.join("bin-bad"));
    assert!(!out.status.success(), "a wrong checksum must fail");
    assert!(String::from_utf8_lossy(&out.stderr).contains("checksum mismatch"));
    assert!(
        !root.join("bin-bad/shellcraft").exists(),
        "nothing is installed"
    );

    let good = release(&root.join("good"), "9.9.9", false, None);
    let bindir = root.join("bin");
    let out = run(&good, &bindir);
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.status.success(), "{text}");
    let version = Command::new(bindir.join("shellcraft"))
        .arg("--version")
        .output()
        .expect("run");
    assert!(version.status.success(), "the installed binary runs");
    assert!(
        root.join("home/cfg/shellcraft/shellcraft.toml").exists(),
        "default config written: {text}"
    );
    let init = fs::read_to_string(root.join("home/cache/shellcraft/init.zsh")).expect("init");
    assert!(
        init.contains(&bindir.display().to_string()),
        "the init script puts the install folder on PATH"
    );
    let rc = fs::read_to_string(root.join("home/.zshrc")).expect("zshrc");
    assert!(rc.contains("shellcraft"), "the shell is wired: {rc}");
    let _ = fs::remove_dir_all(&root);
}

/// Serve `routes` (path -> body) on a loopback port; returns the base URL.
fn serve(routes: Vec<(String, Vec<u8>)>, requests: usize) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    std::thread::spawn(move || {
        for stream in listener.incoming().take(requests) {
            let mut s = stream.expect("conn");
            let mut buf = [0u8; 2048];
            let n = s.read(&mut buf).expect("read");
            let req = String::from_utf8_lossy(&buf[..n]).into_owned();
            let path = req.split_whitespace().nth(1).unwrap_or("/").to_string();
            let body = routes
                .iter()
                .find(|(p, _)| *p == path)
                .map(|(_, b)| b.clone());
            let head = if body.is_some() {
                "200 OK"
            } else {
                "404 Not Found"
            };
            let body = body.unwrap_or_default();
            let _ = write!(
                s,
                "HTTP/1.1 {head}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = s.write_all(&body);
        }
    });
    format!("http://127.0.0.1:{port}")
}

#[test]
fn update_checks_and_replaces_the_running_binary() {
    if !tools_present() {
        return;
    }
    let root = std::env::temp_dir().join(format!("sc-selfupdate-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("bin")).expect("mkdir");
    // the program that updates itself is a copy, never the test binary
    let me = root.join("bin/shellcraft");
    fs::copy(env!("CARGO_BIN_EXE_shellcraft"), &me).expect("copy");
    let before = sha(&me);
    let dist = release(&root, "99.0.0", false, Some("#!/bin/sh\necho updated-99\n"));
    let routes = |version: &str| {
        let t = triple();
        vec![
            ("/version.txt".to_string(), version.as_bytes().to_vec()),
            (
                format!("/shellcraft-{t}.tar.gz"),
                fs::read(dist.join(format!("shellcraft-{t}.tar.gz"))).expect("tar"),
            ),
            (
                format!("/shellcraft-{t}.tar.gz.sha256"),
                fs::read(dist.join(format!("shellcraft-{t}.tar.gz.sha256"))).expect("sum"),
            ),
        ]
    };
    let run = |url: &str, args: &[&str]| {
        let out = Command::new(&me)
            .env("HOME", root.join("home"))
            .env("XDG_CONFIG_HOME", root.join("home/cfg"))
            .env("XDG_CACHE_HOME", root.join("home/cache"))
            .env("XDG_DATA_HOME", root.join("home/data"))
            .env("SHELLCRAFT_UPDATE_URL", url)
            .arg("update")
            .args(args)
            .output()
            .expect("update");
        format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    };

    // already the latest: nothing changes
    let url = serve(routes("0.0.1\n"), 1);
    let out = run(&url, &["--check"]);
    assert!(out.contains("is the latest"), "{out}");

    // a newer version exists: --check only says so
    let url = serve(routes("99.0.0\n"), 1);
    let out = run(&url, &["--check"]);
    assert!(out.contains("99.0.0 is available"), "{out}");
    assert_eq!(sha(&me), before, "--check changes nothing");

    // update: the binary is replaced by the downloaded, verified one
    let url = serve(routes("99.0.0\n"), 3);
    let out = run(&url, &[]);
    assert!(
        out.contains(&format!("{} -> 99.0.0", env!("CARGO_PKG_VERSION"))),
        "{out}"
    );
    assert!(
        me.exists()
            && Command::new(&me)
                .arg("--version")
                .output()
                .expect("run")
                .status
                .success()
    );

    // back to the real program for the failure cases
    fs::copy(env!("CARGO_BIN_EXE_shellcraft"), &me).expect("restore");
    // a corrupt download is refused and the binary stays
    let mut broken = routes("99.0.0\n");
    broken[2].1 = "0".repeat(64).into_bytes();
    let before = sha(&me);
    let url = serve(broken, 3);
    let out = run(&url, &[]);
    assert!(
        out.contains("checksum mismatch") && sha(&me) == before,
        "{out}"
    );

    // an unreachable site is a note, not a failure
    let out = run("http://127.0.0.1:9", &["--check"]);
    assert!(
        out.contains("could not check for a newer shellcraft"),
        "{out}"
    );
    let _ = fs::remove_dir_all(&root);
}
