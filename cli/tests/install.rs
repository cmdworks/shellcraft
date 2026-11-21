#![cfg(feature = "installer")]
use std::process::Command;

#[test]
fn dry_run_prints_package_manager_command() {
    let dir = std::env::temp_dir().join(format!("sc-install-cli-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("mkdir");
    std::fs::write(dir.join("brew"), "").expect("fake brew");
    std::fs::write(dir.join("zoxide"), "").expect("fake installed tool");
    let out = Command::new(env!("CARGO_BIN_EXE_shellcraft"))
        .env("PATH", &dir)
        .args(["install", "--dry-run", "fd", "zoxide", "ripgrep"])
        .output()
        .expect("spawn");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(text.contains("fd: brew install fd"), "{text}");
    assert!(text.contains("zoxide: already installed"), "{text}");
    assert!(text.contains("ripgrep: brew install ripgrep"), "{text}");
    let bad = Command::new(env!("CARGO_BIN_EXE_shellcraft"))
        .env("PATH", &dir)
        .args(["install", "--dry-run", "nosuchtool"])
        .output()
        .expect("spawn");
    assert!(!bad.status.success());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn native_dry_run_names_the_release_asset() {
    let out = Command::new(env!("CARGO_BIN_EXE_shellcraft"))
        .env("PATH", "")
        .args(["install", "--native", "--dry-run", "starship", "fzf"])
        .output()
        .expect("spawn");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("starship: download https://github.com/starship/starship/releases/latest/download/starship-"), "{text}");
    assert!(text.contains(".tar.gz (+ .sha256)"), "{text}");
    assert!(!out.status.success(), "fzf has no native download");
    assert!(String::from_utf8_lossy(&out.stderr).contains("fzf:"));
}
