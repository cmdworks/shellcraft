use std::path::PathBuf;
use std::process::{Command, Output};

struct Env {
    dir: PathBuf,
}

impl Env {
    fn new(name: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("shellcraft-apply-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        Self { dir }
    }
    fn cfg(&self) -> PathBuf {
        self.dir.join("shellcraft.toml")
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_shellcraft"))
            .args(args)
            .env("HOME", &self.dir)
            .env("SHELLCRAFT_CONFIG", self.cfg())
            .env("XDG_CACHE_HOME", self.dir.join("cache"))
            .env_remove("ZDOTDIR")
            .env_remove("XDG_CONFIG_HOME") // CI runners set it; the rc paths must come from HOME
            .env_remove("SHELL")
            .output()
            .expect("run shellcraft")
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn text(o: &[u8]) -> String {
    String::from_utf8_lossy(o).into_owned()
}

#[test]
fn print_zsh_uses_config() {
    let e = Env::new("print");
    std::fs::write(e.cfg(), "[env]\nEDITOR = \"nvim\"\n").unwrap();
    let out = e.run(&["apply", "--shell", "zsh", "--print"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(text(&out.stdout).contains("export EDITOR='nvim'"));
}

#[test]
fn missing_config_points_to_init() {
    let e = Env::new("missing");
    let out = e.run(&["apply", "--shell", "zsh"]);
    assert!(!out.status.success());
    assert!(text(&out.stderr).contains("shellcraft init"));
}

#[test]
fn invalid_config_names_the_file() {
    let e = Env::new("invalid");
    std::fs::write(e.cfg(), "[shell]\nhistory_size = 0\n").unwrap();
    let out = e.run(&["apply", "--shell", "bash"]);
    assert!(!out.status.success());
    let err = text(&out.stderr);
    assert!(
        err.contains("shellcraft.toml") && err.contains("history_size"),
        "{err}"
    );
}

#[test]
fn nu_and_pwsh_apply_with_rc() {
    for (shell, rc, init) in [
        ("nu", ".config/nushell/config.nu", "init.nu"),
        (
            "pwsh",
            ".config/powershell/Microsoft.PowerShell_profile.ps1",
            "init.ps1",
        ),
    ] {
        let e = Env::new(shell);
        std::fs::write(e.cfg(), "").unwrap();
        let out = e.run(&["apply", "--shell", shell, "--rc"]);
        assert!(out.status.success(), "{shell}: {}", text(&out.stderr));
        let text_rc = std::fs::read_to_string(e.dir.join(rc)).unwrap();
        assert!(
            text_rc.contains("# >>> shellcraft >>>") && text_rc.contains(init),
            "{text_rc}"
        );
        assert!(e.dir.join("cache/shellcraft").join(init).exists());
    }
}

#[test]
fn no_shell_info_is_an_error() {
    let e = Env::new("noshell");
    std::fs::write(e.cfg(), "").unwrap();
    assert!(!e.run(&["apply"]).status.success());
}

#[test]
fn rc_wiring_is_idempotent_and_backs_up() {
    let e = Env::new("rc");
    std::fs::write(e.cfg(), "").unwrap();
    let rc = e.dir.join(".zshrc");
    std::fs::write(&rc, "export KEEP=1\n").unwrap();

    let out = e.run(&["apply", "--shell", "zsh", "--rc"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let wired = std::fs::read_to_string(&rc).unwrap();
    assert!(wired.starts_with("export KEEP=1\n"));
    assert!(wired.contains("# >>> shellcraft >>>") && wired.contains("init.zsh"));
    assert!(e.dir.join("cache/shellcraft/init.zsh").exists());
    assert!(e.dir.join("cache/shellcraft/config.bin").exists());
    let backups = std::fs::read_dir(&e.dir)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|f| f.file_name().to_string_lossy().contains(".pre-shellcraft-"))
        .count();
    assert_eq!(backups, 1);

    let again = e.run(&["apply", "--shell", "zsh", "--rc"]);
    assert!(text(&again.stdout).contains("already wired"));
    assert_eq!(std::fs::read_to_string(&rc).unwrap(), wired);
}

#[test]
fn wired_zsh_starts_and_loads_config() {
    let e = Env::new("real");
    std::fs::write(e.cfg(), "[env]\nSC_TEST = \"hello\"\n").unwrap();
    assert!(e.run(&["apply", "--shell", "zsh", "--rc"]).status.success());
    if Command::new("zsh").arg("--version").output().is_err() {
        return;
    }
    let out = Command::new("zsh")
        .args(["-i", "-c", "print -r -- $SC_TEST"])
        .env("ZDOTDIR", &e.dir)
        .env("HOME", &e.dir)
        .env("XDG_CACHE_HOME", e.dir.join("cache"))
        .output()
        .expect("run zsh");
    assert!(
        text(&out.stdout).contains("hello"),
        "{} {}",
        text(&out.stdout),
        text(&out.stderr)
    );
}

#[test]
fn uninstall_restores_rc_and_keeps_config() {
    let e = Env::new("uninstall");
    std::fs::write(e.cfg(), "").unwrap();
    let rc = e.dir.join(".zshrc");
    std::fs::write(&rc, "export KEEP=1\n").unwrap();
    assert!(e.run(&["apply", "--shell", "zsh", "--rc"]).status.success());
    assert!(e.dir.join("cache/shellcraft/init.zsh").exists());

    let out = e.run(&["uninstall"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(std::fs::read_to_string(&rc).unwrap(), "export KEEP=1\n");
    assert!(!e.dir.join("cache/shellcraft/init.zsh").exists());
    assert!(e.cfg().exists());
}

#[test]
fn uninstall_purge_removes_cache_dir() {
    let e = Env::new("purge");
    std::fs::write(e.cfg(), "").unwrap();
    assert!(e.run(&["apply", "--shell", "bash"]).status.success());
    let cache = e.dir.join("cache/shellcraft");
    assert!(cache.exists());
    assert!(e.run(&["uninstall", "--purge"]).status.success());
    assert!(!cache.exists());
    assert!(e.cfg().exists());
}

#[test]
fn doctor_flow() {
    let e = Env::new("doctor");
    std::fs::write(e.cfg(), "").unwrap();
    let out = e.run(&["doctor", "--shell", "zsh"]);
    assert!(!out.status.success());
    assert!(text(&out.stdout).contains("init script missing"));

    assert!(e.run(&["apply", "--shell", "zsh", "--rc"]).status.success());
    let out = e.run(&["doctor", "--shell", "zsh"]);
    assert!(
        out.status.success(),
        "{} {}",
        text(&out.stdout),
        text(&out.stderr)
    );
    assert!(text(&out.stdout).contains("all good"));

    std::fs::write(e.cfg(), "[env]\nA = \"1\"\n").unwrap();
    let out = e.run(&["doctor", "--shell", "zsh"]);
    assert!(!out.status.success());
    assert!(text(&out.stdout).contains("stale"));
}

#[test]
fn doctor_reports_invalid_config() {
    let e = Env::new("doctor-bad");
    std::fs::write(e.cfg(), "[shell]\nhistory_size = 0\n").unwrap();
    let out = e.run(&["doctor"]);
    assert!(!out.status.success());
    assert!(text(&out.stdout).contains("history_size"));
}

#[test]
fn bench_and_stats_run() {
    let e = Env::new("bench");
    std::fs::write(e.cfg(), "[tools]\nnonexistent-tool = true\n").unwrap();
    let out = e.run(&["bench", "2", "--shell", "bash"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(text(&out.stdout).contains("overhead"));
    let out = e.run(&["stats", "--shell", "bash"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let t = text(&out.stdout);
    assert!(
        t.contains("Speed") && t.contains("Disk") && t.contains("nonexistent-tool"),
        "{t}"
    );
}

#[test]
fn fish_apply_writes_config_fish_block() {
    let e = Env::new("fishrc");
    std::fs::write(e.cfg(), "").unwrap();
    let out = e.run(&["apply", "--shell", "fish", "--rc"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let rc = std::fs::read_to_string(e.dir.join(".config/fish/config.fish")).unwrap();
    assert!(
        rc.contains("# >>> shellcraft >>>") && rc.contains("; and source"),
        "{rc}"
    );
    assert!(e.dir.join("cache/shellcraft/init.fish").exists());
    assert!(e.run(&["uninstall"]).status.success());
    let rc = std::fs::read_to_string(e.dir.join(".config/fish/config.fish")).unwrap();
    assert!(!rc.contains("shellcraft"));
}

/// Wire a shell with `apply --rc`, start it for real and read a variable from the config.
fn wired_shell_loads_config(shell: &str, args: &[&str]) {
    if Command::new(shell).arg("--version").output().is_err() {
        return; // shell not installed
    }
    let e = Env::new(&format!("wired-{shell}"));
    std::fs::write(e.cfg(), "[env]\nSC_TEST = \"hello world\"\n").expect("write config");
    let out = e.run(&["apply", "--shell", shell, "--rc"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let out = Command::new(shell)
        .args(args)
        .env("HOME", &e.dir)
        .env("XDG_CACHE_HOME", e.dir.join("cache"))
        .env_remove("XDG_CONFIG_HOME")
        .output()
        .expect("run shell");
    assert!(
        text(&out.stdout).contains("hello world"),
        "{shell}: {} {}",
        text(&out.stdout),
        text(&out.stderr)
    );
}

#[test]
fn wired_bash_loads_config() {
    wired_shell_loads_config("bash", &["-i", "-c", "echo $SC_TEST"]);
}

#[test]
fn wired_fish_loads_config() {
    wired_shell_loads_config("fish", &["-i", "-c", "echo $SC_TEST"]);
}
