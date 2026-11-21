use std::process::Command;

fn prompt(args: &[&str], cache: &std::path::Path, cwd: &std::path::Path) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_shellcraft"))
        .env("XDG_CACHE_HOME", cache)
        .env("HOME", cache)
        .env("SHELLCRAFT_CONFIG", cache.join("none.toml"))
        .current_dir(cwd)
        .arg("prompt")
        .args(args)
        .output()
        .expect("spawn shellcraft");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn renders_for_each_shell_with_defaults() {
    let root = std::env::temp_dir().join(format!("sc-prompt-cli-{}", std::process::id()));
    let repo = root.join("repo");
    std::fs::create_dir_all(repo.join(".git")).expect("mkdir");
    std::fs::write(repo.join(".git/HEAD"), "ref: refs/heads/topic\n").expect("write");

    let z = prompt(&["--shell", "zsh"], &root, &repo);
    assert!(
        z.contains("%{") && z.contains("topic") && z.contains("repo"),
        "{z:?}"
    );
    let segs = prompt(&["--shell", "zsh", "--segments-only"], &root, &repo);
    assert!(
        segs.starts_with(' ')
            && segs.contains("topic")
            && !segs.contains("repo")
            && z.contains(&segs),
        "{segs:?}"
    );
    let b = prompt(&["--shell", "bash", "--status", "3"], &root, &repo);
    assert!(b.contains("\\[") && b.contains("[3]"), "{b:?}");
    let f = prompt(
        &[
            "--shell",
            "fish",
            "--duration",
            "9000",
            "--remote",
            "me@box",
        ],
        &root,
        &repo,
    );
    assert!(f.contains("9s") && f.contains("me@box"), "{f:?}");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn unknown_shell_fails() {
    let root = std::env::temp_dir();
    let out = Command::new(env!("CARGO_BIN_EXE_shellcraft"))
        .args(["prompt", "--shell", "tcsh"])
        .env("HOME", &root)
        .output()
        .expect("spawn");
    assert!(!out.status.success());
}

/// The zsh init script draws everything except the plugin segments itself; for every directory,
/// status, duration and SSH case it must produce exactly what `shellcraft prompt` produces with no
/// segments configured (skipped when zsh is not installed).
#[test]
fn zsh_prompt_matches_the_renderer_byte_for_byte() {
    if Command::new("zsh").arg("--version").output().is_err() {
        return;
    }
    let root = std::env::temp_dir().join(format!("sc-zprompt-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("home/a/b/c/d/e")).expect("mkdir");
    std::fs::create_dir_all(root.join("cfg/shellcraft")).expect("mkdir");
    // no symlinks in the path: the renderer sees the physical directory, the shell the logical one
    let root = std::fs::canonicalize(&root).expect("canonical");
    std::fs::write(
        root.join("cfg/shellcraft/shellcraft.toml"),
        "[prompt]\nsegments = []\nsymbol = \"%>\"\nmin_duration_ms = 1500\n[theme]\nname = \"default\"\n",
    )
    .expect("config");
    let bin = std::path::Path::new(env!("CARGO_BIN_EXE_shellcraft"));
    let run = |args: &[&str]| {
        Command::new(bin)
            .env("HOME", root.join("home"))
            .env("XDG_CONFIG_HOME", root.join("cfg"))
            .env("XDG_CACHE_HOME", root.join("cache"))
            .env("XDG_DATA_HOME", root.join("data"))
            .args(args)
            .output()
            .expect("run")
    };
    assert!(run(&["apply", "--shell", "zsh"]).status.success());
    let script = r#"
source $XDG_CACHE_HOME/shellcraft/init.zsh 2>/dev/null
bad=0; n=0
for dir in $HOME $HOME/a $HOME/a/b/c $HOME/a/b/c/d/e / /usr/lib /usr; do
  cd $dir || continue
  for ssh in 0 1; do
    if (( ssh )); then export SSH_CONNECTION="1 2 3 4"; remote=(--remote "${(%):-%n@%m}"); else unset SSH_CONNECTION; remote=(); fi
    for st in 0 1 130; do
      for ms in 0 1499 1500 3000 61000 3700000; do
        want=$(command shellcraft prompt --shell zsh --status $st --duration $ms $remote)
        _sc_p_build $st $ms
        got="$_sc_p_head$_sc_p_tail"
        n=$((n+1))
        [[ $got == $want ]] || { bad=$((bad+1)); print -r -- "DIFF dir=$dir ssh=$ssh st=$st ms=$ms"; print -r -- " want=${(q+)want}"; print -r -- " got =${(q+)got}"; }
      done
    done
  done
done
print -r -- "checked=$n bad=$bad"
"#;
    let out = Command::new("zsh")
        .args(["-f", "-c", script])
        .env("HOME", root.join("home"))
        .env("XDG_CONFIG_HOME", root.join("cfg"))
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("XDG_DATA_HOME", root.join("data"))
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", bin.parent().expect("dir").display()),
        )
        .output()
        .expect("zsh");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("bad=0") && text.contains("checked=252"),
        "{text}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// Same parity check for fish: the prompt fish draws itself equals `shellcraft prompt` without
/// segments (skipped when fish is not installed).
#[test]
fn fish_prompt_matches_the_renderer_byte_for_byte() {
    if Command::new("fish").arg("--version").output().is_err() {
        return;
    }
    let root = std::env::temp_dir().join(format!("sc-fprompt-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("home/a/b/c/d/e")).expect("mkdir");
    std::fs::create_dir_all(root.join("cfg/shellcraft")).expect("mkdir");
    let root = std::fs::canonicalize(&root).expect("canonical");
    std::fs::write(
        root.join("cfg/shellcraft/shellcraft.toml"),
        "[prompt]\nsegments = []\nsymbol = \"%>\"\nmin_duration_ms = 1500\n",
    )
    .expect("config");
    let bin = std::path::Path::new(env!("CARGO_BIN_EXE_shellcraft"));
    let envs = |c: &mut Command| {
        c.env("HOME", root.join("home"))
            .env("XDG_CONFIG_HOME", root.join("cfg"))
            .env("XDG_CACHE_HOME", root.join("cache"))
            .env("XDG_DATA_HOME", root.join("data"))
            .env(
                "PATH",
                format!(
                    "{}:/usr/bin:/bin:/opt/homebrew/bin",
                    bin.parent().expect("dir").display()
                ),
            );
    };
    let mut apply = Command::new(bin);
    envs(&mut apply);
    assert!(apply
        .args(["apply", "--shell", "fish"])
        .output()
        .expect("apply")
        .status
        .success());
    let script = format!(
        r#"
source {init}
set bad 0; set n 0
for dir in $HOME $HOME/a $HOME/a/b/c $HOME/a/b/c/d/e / /usr/lib /usr
  cd $dir; or continue
  for ssh in 0 1
    if test $ssh -eq 1
      set -gx SSH_CONNECTION "1 2 3 4"; set remote --remote $USER@(prompt_hostname)
    else
      set -e SSH_CONNECTION; set remote
    end
    for st in 0 1 130
      for ms in 0 1499 1500 3000 61000 3700000
        set want (command shellcraft prompt --shell fish --status $st --duration $ms $remote | string collect)
        _sc_p_build $st $ms
        set got (printf '%s%s' "$_sc_p_head" "$_sc_p_tail" | string collect)
        set n (math $n + 1)
        if test "$got" != "$want"
          set bad (math $bad + 1)
          echo "DIFF dir=$dir ssh=$ssh st=$st ms=$ms"
          echo " want="(string escape -- $want)
          echo " got ="(string escape -- $got)
        end
      end
    end
  end
end
echo "checked=$n bad=$bad"
"#,
        init = root.join("cache/shellcraft/init.fish").display()
    );
    let mut fish = Command::new("fish");
    envs(&mut fish);
    let out = fish
        .args(["--no-config", "-i", "-c", &script])
        .output()
        .expect("fish");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        text.contains("bad=0") && text.contains("checked=252"),
        "{text}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn layout_theme_draws_parts_and_both_sides() {
    let home = &std::env::temp_dir().join(format!("sc-layout-{}", std::process::id()));
    std::fs::create_dir_all(home).unwrap();
    let run = |args: &[&str]| {
        let o = Command::new(env!("CARGO_BIN_EXE_shellcraft"))
            .args(args)
            .env("HOME", home)
            .env("XDG_CONFIG_HOME", home.join("c"))
            .env("XDG_DATA_HOME", home.join("d"))
            .env("XDG_CACHE_HOME", home.join("k"))
            .env_remove("NVM_BIN")
            .env_remove("VIRTUAL_ENV")
            .current_dir(home)
            .output()
            .unwrap();
        assert!(
            o.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&o.stderr)
        );
        String::from_utf8_lossy(&o.stdout).into_owned()
    };
    run(&["init"]);
    run(&["install", "passionp"]);
    run(&["theme", "passionp"]);
    let out = run(&[
        "prompt", "--shell", "zsh", "--both", "--jobs", "2", "--status", "1", "--root",
    ]);
    let (left, right) = out.trim_end().split_once('\x1f').expect("left and right");
    assert!(
        left.contains("ROOT") && left.contains('✘') || left.contains("\u{f00d}"),
        "{left:?}"
    );
    assert!(right.contains("2"), "jobs on the right: {right:?}");
    run(&["apply", "--shell", "zsh"]);
    let init = std::fs::read_to_string(home.join("k/shellcraft/init.zsh")).unwrap();
    assert!(
        init.contains("--both"),
        "layout themes use the async full prompt"
    );
    let _ = std::fs::remove_dir_all(home);
}
