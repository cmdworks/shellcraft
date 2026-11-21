#![cfg(feature = "coreutils")]
use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_shellcraft"))
}

#[test]
fn cat_is_byte_exact_when_piped_and_reads_stdin() {
    let dir = std::env::temp_dir().join(format!("sc-cat-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("mkdir");
    let a = dir.join("a.txt");
    let data = b"line one\n\x00binary\xff\nno newline";
    std::fs::write(&a, data).expect("write");
    let out = bin().arg("cat").arg(&a).output().expect("spawn");
    assert_eq!(out.stdout, data, "piped output must be unchanged");
    let missing = bin()
        .arg("cat")
        .arg(dir.join("nope"))
        .output()
        .expect("spawn");
    assert!(!missing.status.success());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn ls_plain_when_piped() {
    let dir = std::env::temp_dir().join(format!("sc-ls-cli-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("sub")).expect("mkdir");
    std::fs::write(dir.join("f"), "x").expect("write");
    std::fs::write(dir.join(".h"), "x").expect("write");
    let out = bin().arg("ls").arg(&dir).output().expect("spawn");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "sub\nf\n",
        "one per line, no colour, dirs first"
    );
    let all = bin().args(["ls", "-a"]).arg(&dir).output().expect("spawn");
    assert_eq!(String::from_utf8_lossy(&all.stdout).lines().count(), 3);
    assert!(!bin()
        .args(["ls", "/definitely/not/here"])
        .output()
        .expect("spawn")
        .status
        .success());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn closed_pipe_exits_quietly() {
    // Commands that use `println!` (like `plugin list`) must not print a panic message when the
    // reader goes away, as in `shellcraft plugin list | head -1`.
    let mut child = bin()
        .args(["plugin", "list"])
        .env("HOME", env!("CARGO_MANIFEST_DIR"))
        .env("XDG_CONFIG_HOME", env!("CARGO_MANIFEST_DIR"))
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn");
    drop(child.stdout.take()); // close the read end at once
    let out = child.wait_with_output().expect("wait");
    assert!(
        !String::from_utf8_lossy(&out.stderr).contains("panicked"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}
