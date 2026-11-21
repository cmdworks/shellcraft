use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

fn run(root: &Path, cwd: &Path, args: &[&str]) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_shellcraft"))
        .env("HOME", root)
        .env("XDG_CONFIG_HOME", root.join("cfg"))
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("XDG_DATA_HOME", root.join("data"))
        .env("SHELLCRAFT_NO_BG", "1")
        .env_remove("SHELLCRAFT_CONFIG")
        .current_dir(cwd)
        .args(args)
        .output()
        .expect("spawn");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr),
    )
}

fn plain(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            for n in chars.by_ref() {
                if n.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[test]
fn external_segment_and_plugin_list() {
    let root = std::env::temp_dir().join(format!("sc-plugin-cli-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let proj = root.join("proj");
    fs::create_dir_all(&proj).expect("mkdir");
    let exe = root.join("seg.sh");
    fs::write(&exe, "#!/bin/sh\necho '[warn] 3 todos'\n").expect("write");
    fs::set_permissions(&exe, fs::Permissions::from_mode(0o755)).expect("chmod");
    fs::create_dir_all(root.join("cfg/shellcraft")).expect("mkdir");
    fs::write(
        root.join("cfg/shellcraft/shellcraft.toml"),
        format!(
            "[prompt]\nsegments = [\"todos\", \"git\", \"versions\"]\n[plugin.todos]\nsegment = \"{}\"\n[plugin.git]\nenabled = false\n[plugin.versions.options]\necosystems = \"go\"\n",
            exe.display()
        ),
    )
    .expect("config");
    fs::write(proj.join("go.mod"), "go 1.22\n").expect("write");
    assert!(run(&root, &proj, &["apply", "--shell", "zsh"]).0);

    // nothing cached yet: the external segment is empty, versions shows the declared go version
    let (_, first) = run(&root, &proj, &["prompt", "--shell", "fish"]);
    assert!(!plain(&first).contains("todos"), "{first:?}");
    assert!(plain(&first).contains("go 1.22"), "{first:?}");

    // the prompt keys its cache by the resolved cwd (macOS: /private/var/...)
    let real = fs::canonicalize(&proj).expect("canonicalize");
    let real = real.to_str().expect("utf8");
    assert!(
        run(
            &root,
            &proj,
            &["plugin", "refresh", "segment", "todos", real]
        )
        .0
    );
    let (_, second) = run(&root, &proj, &["prompt", "--shell", "fish"]);
    assert!(plain(&second).contains("3 todos"), "{second:?}");
    assert!(
        second.contains("\u{1b}[33m3 todos"),
        "warn tone uses the theme colour: {second:?}"
    );

    let (ok, list) = run(&root, &proj, &["plugin", "list"]);
    assert!(ok);
    assert!(
        list.lines()
            .any(|l| l.starts_with("git") && l.contains(" no ")),
        "{list}"
    );
    assert!(
        list.lines()
            .any(|l| l.starts_with("todos") && l.contains("external")),
        "{list}"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn unknown_segment_is_reported() {
    let root = std::env::temp_dir().join(format!("sc-plugin-unknown-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("cfg/shellcraft")).expect("mkdir");
    fs::write(
        root.join("cfg/shellcraft/shellcraft.toml"),
        "[prompt]\nsegments = [\"ghost\"]\n",
    )
    .expect("config");
    let (ok, out) = run(&root, &root, &["plugin", "list"]);
    assert!(ok && out.contains("`ghost` matches no plugin"), "{out}");
    let _ = fs::remove_dir_all(&root);
}
