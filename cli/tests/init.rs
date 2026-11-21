use std::path::Path;
use std::process::{Command, Output};

fn run(cfg: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_shellcraft"))
        .args(args)
        .env("SHELLCRAFT_CONFIG", cfg)
        .output()
        .expect("run shellcraft")
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("shellcraft-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

#[test]
fn init_creates_then_refuses_then_forces() {
    let dir = temp_dir("init");
    let cfg = dir.join("sub").join("shellcraft.toml");

    let out = run(&cfg, &["init"]);
    assert!(out.status.success());
    let text = std::fs::read_to_string(&cfg).unwrap();
    assert!(text.contains("[shell]"));

    std::fs::write(&cfg, "# mine\n").unwrap();
    let out = run(&cfg, &["init"]);
    assert!(!out.status.success());
    assert_eq!(std::fs::read_to_string(&cfg).unwrap(), "# mine\n");

    let out = run(&cfg, &["init", "--force"]);
    assert!(out.status.success());
    assert!(std::fs::read_to_string(&cfg).unwrap().contains("[shell]"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn path_prints_override() {
    let out = run(Path::new("/tmp/x/c.toml"), &["path"]);
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "/tmp/x/c.toml");
}

#[test]
fn completions_for_each_shell() {
    for (shell, needle) in [
        ("zsh", "#compdef shellcraft"),
        ("bash", "_shellcraft"),
        ("fish", "complete -c shellcraft"),
    ] {
        let out = run(Path::new("/tmp/unused.toml"), &["completions", shell]);
        assert!(out.status.success(), "{shell}");
        assert!(
            String::from_utf8_lossy(&out.stdout).contains(needle),
            "{shell}"
        );
    }
}
