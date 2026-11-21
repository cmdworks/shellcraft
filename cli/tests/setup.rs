use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn setup_writes_config_and_wires_rc() {
    let home = std::env::temp_dir().join(format!("sc-setup-cli-{}", std::process::id()));
    std::fs::create_dir_all(&home).expect("mkdir");
    let mut child = Command::new(env!("CARGO_BIN_EXE_shellcraft"))
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", home.join("cfg"))
        .env("XDG_CACHE_HOME", home.join("cache"))
        .env_remove("SHELLCRAFT_CONFIG")
        .arg("setup")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(b"bash\ndefault\ny\nnone\ny\n")
        .expect("answers");
    let out = child.wait_with_output().expect("wait");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let cfg = std::fs::read_to_string(home.join("cfg/shellcraft/shellcraft.toml")).expect("config");
    assert!(cfg.contains("default = \"bash\"") && cfg.contains("builtin = true"));
    // a theme that is not installed is refused with a pointer to the theme plugin
    let mut bad = Command::new(env!("CARGO_BIN_EXE_shellcraft"))
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", home.join("cfg2"))
        .env("XDG_CACHE_HOME", home.join("cache2"))
        .env("XDG_DATA_HOME", home.join("data2"))
        .arg("setup")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn");
    bad.stdin
        .take()
        .expect("stdin")
        .write_all(b"bash\ndracula\n")
        .expect("answers");
    let out = bad.wait_with_output().expect("wait");
    assert!(
        !out.status.success() && String::from_utf8_lossy(&out.stderr).contains("plugin add themes")
    );
    let rc = std::fs::read_to_string(home.join(".bashrc")).expect("rc");
    assert!(rc.contains("# >>> shellcraft >>>"), "{rc}");
    let _ = std::fs::remove_dir_all(&home);
}
