use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

fn run(root: &Path, path: &str, args: &[&str]) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_shellcraft"))
        .env("HOME", root)
        .env("XDG_CONFIG_HOME", root.join("cfg"))
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("XDG_DATA_HOME", root.join("data"))
        .env("SHELL", "/bin/zsh")
        .env("PATH", path)
        .env_remove("SHELLCRAFT_CONFIG")
        .args(args)
        .output()
        .expect("spawn");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

#[test]
fn complete_flags_subcommands_config_and_clear() {
    let root = std::env::temp_dir().join(format!("sc-complete-cli-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let bin = root.join("bin");
    fs::create_dir_all(&bin).expect("mkdir");
    fs::create_dir_all(root.join("cfg/shellcraft")).expect("mkdir");
    let tool = bin.join("mytool");
    fs::write(&tool, "#!/bin/sh\nprintf 'Commands:\\n  deploy  Ship it\\n  destroy  Tear down\\n\\nOptions:\\n  -q, --quiet  Be quiet\\n'\n").expect("script");
    fs::set_permissions(&tool, fs::Permissions::from_mode(0o755)).expect("chmod");
    let skipme = bin.join("skipme");
    fs::write(
        &skipme,
        "#!/bin/sh\ntouch RAN\nprintf 'Commands:\\n  go  Go\\n'\n",
    )
    .expect("script");
    fs::set_permissions(&skipme, fs::Permissions::from_mode(0o755)).expect("chmod");
    let path = format!("{}:/usr/bin:/bin", bin.display());

    let cfg = root.join("cfg/shellcraft/shellcraft.toml");
    fs::write(&cfg, "[completion]\nskip = [\"skipme\"]\n").expect("config");
    assert!(run(&root, &path, &["apply", "--shell", "zsh"]).0);

    let (ok, out) = run(&root, &path, &["complete", "--", "mytool", "de"]);
    assert!(ok);
    assert_eq!(out, "deploy\tShip it\ndestroy\tTear down\n");
    let (_, flags) = run(&root, &path, &["complete", "--", "mytool", "--q"]);
    assert_eq!(flags, "--quiet\tBe quiet\n");
    assert!(
        run(&root, &path, &["complete", "--", "skipme", ""])
            .1
            .is_empty(),
        "skip list is honoured"
    );
    assert!(!root.join("RAN").exists() && !Path::new("RAN").exists());
    assert!(run(&root, &path, &["complete", "--", "nosuchcommand", ""])
        .1
        .is_empty());

    // the learned spec is cached on disk; --clear forgets it
    assert!(root
        .join("cache/shellcraft/completions/mytool.tsv")
        .exists());
    assert!(run(&root, &path, &["complete", "--clear"])
        .1
        .contains("cleared"));
    assert!(!root.join("cache/shellcraft/completions").exists());

    // switching it off in the config stops all of it
    fs::write(&cfg, "[completion]\nhelp = false\n").expect("config");
    assert!(run(&root, &path, &["apply", "--shell", "zsh"]).0);
    assert!(run(&root, &path, &["complete", "--", "mytool", "de"])
        .1
        .is_empty());
    let init = fs::read_to_string(root.join("cache/shellcraft/init.zsh")).expect("init");
    assert!(
        !init.contains("_sc_help_complete"),
        "no completer when disabled"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn generated_scripts_wire_the_completer() {
    let root = std::env::temp_dir().join(format!("sc-complete-wire-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("cfg/shellcraft")).expect("mkdir");
    assert!(run(&root, "/usr/bin:/bin", &["init"]).0);
    let zsh = run(
        &root,
        "/usr/bin:/bin",
        &["apply", "--print", "--shell", "zsh"],
    )
    .1;
    assert!(zsh.contains("zstyle ':completion:*' completer _sc_help_complete _complete _ignored"));
    assert!(
        zsh.contains("_comps[$words[1]]"),
        "defers to commands that have their own completion"
    );
    let bash = run(
        &root,
        "/usr/bin:/bin",
        &["apply", "--print", "--shell", "bash"],
    )
    .1;
    assert!(bash.contains("complete -o bashdefault -o default -D -F _sc_default_complete"));
    assert!(
        bash.contains("_completion_loader"),
        "stays out of bash-completion's way"
    );
    let _ = fs::remove_dir_all(&root);
}
