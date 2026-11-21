//! The zsh line editor (suggestion and colours) runs inside the shell; these tests drive its
//! functions in a real zsh (skipped when zsh is not installed). `hist recent` feeds its memory.

use std::fs;
use std::path::Path;
use std::process::Command;

fn env<'a>(cmd: &'a mut Command, root: &Path) -> &'a mut Command {
    cmd.env("HOME", root)
        .env("XDG_CONFIG_HOME", root.join("cfg"))
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("XDG_DATA_HOME", root.join("data"))
}

fn sc(root: &Path, args: &[&str]) -> (bool, String) {
    let out = env(&mut Command::new(env!("CARGO_BIN_EXE_shellcraft")), root)
        .args(args)
        .output()
        .expect("run");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

fn zsh(root: &Path, code: &str) -> String {
    let script = format!(
        "source {}/cache/shellcraft/init.zsh 2>/dev/null; {code}",
        root.display()
    );
    let bin = Path::new(env!("CARGO_BIN_EXE_shellcraft"))
        .parent()
        .expect("dir");
    let out = env(&mut Command::new("zsh"), root)
        .args(["-f", "-c", &script])
        .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
        .output()
        .expect("zsh");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn recent_lists_successful_single_line_commands_newest_first() {
    let root = std::env::temp_dir().join(format!("sc-recent-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("mkdir");
    for (exit, cwd, cmd) in [
        ("0", "/a", "git status"),
        ("1", "/a", "git bad"),
        ("0", "/b", "ls"),
        ("0", "/a", "git status"),
    ] {
        assert!(
            sc(
                &root,
                &["hist", "add", "--exit", exit, "--cwd", cwd, "--", cmd]
            )
            .0
        );
    }
    let (ok, out) = sc(&root, &["hist", "recent"]);
    assert!(ok);
    assert_eq!(
        out, "/a\tgit status\n/b\tls\n",
        "each pair once, newest first, failures left out"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn zsh_suggests_and_colours_without_starting_a_process() {
    if Command::new("zsh").arg("--version").output().is_err() {
        return;
    }
    let root = std::env::temp_dir().join(format!("sc-hl-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("mkdir");
    assert!(sc(&root, &["init"]).0);
    assert!(sc(&root, &["apply", "--shell", "zsh"]).0);
    assert!(
        sc(
            &root,
            &["hist", "add", "--cwd", "/work", "--", "git status --short"]
        )
        .0
    );
    assert!(
        sc(
            &root,
            &["hist", "add", "--cwd", "/other", "--", "git stash"]
        )
        .0
    );

    // colours (offsets in characters)
    let text = zsh(
        &root,
        r#"BUFFER='ls -l "a b" | nosuchcmd_zz $HOME x'; CURSOR=${#BUFFER}; _sc_line_update; print -l -- $region_highlight"#,
    );
    for want in [
        "0 2 fg=green,bold", // ls exists
        "3 5 fg=cyan",       // -l
        "6 11 fg=yellow",    // "a b"
        "12 13 fg=blue",     // |
        "14 26 fg=red",      // nosuchcmd_zz does not exist
        "27 32 fg=magenta",  // $HOME
    ] {
        assert!(text.contains(want), "{want} missing in:\n{text}");
    }
    // the word still being typed gets no verdict; a comment is dimmed to the end
    let text = zsh(
        &root,
        r#"BUFFER='nosuchcm'; CURSOR=${#BUFFER}; _sc_line_update; print -l -- $region_highlight"#,
    );
    assert!(!text.contains("fg=red"), "{text}");
    let text = zsh(
        &root,
        r#"BUFFER='ls # note'; CURSOR=${#BUFFER}; _sc_line_update; print -l -- $region_highlight"#,
    );
    assert!(text.contains("3 9 fg=8"), "{text}");
    let text = zsh(
        &root,
        r#"BUFFER='FOO=bar sudo -E make'; CURSOR=${#BUFFER}; _sc_line_update; print -l -- $region_highlight"#,
    );
    for want in ["0 3 fg=magenta", "8 12 fg=green,bold", "13 15 fg=cyan"] {
        assert!(text.contains(want), "{want} missing in:\n{text}"); // assignment, sudo, its option
    }

    // suggestion: this directory first, then anywhere; never the typed text itself; not for failures
    let sug = |cwd: &str, buf: &str| {
        zsh(
            &root,
            &format!(
                r#"cd {cwd} 2>/dev/null; PWD={cwd}; BUFFER='{buf}'; CURSOR=${{#BUFFER}}; _sc_line_update; print -r -- "[$POSTDISPLAY]""#
            ),
        )
    };
    assert_eq!(sug("/work", "git s").trim(), "[tatus --short]");
    assert_eq!(sug("/other", "git s").trim(), "[tash]");
    assert_eq!(sug("/work", "git status --short").trim(), "[]");
    assert_eq!(
        sug("/work", "git %").trim(),
        "[]",
        "glob characters are literal"
    );
    let added = zsh(
        &root,
        r#"BUFFER='git sw'; _sc_suggest; print -r -- "[$REPLY]"; _sc_cache_add 0 /work 'git switch main'; _sc_suggest; print -r -- "[$REPLY]"; _sc_cache_add 1 /work 'git swfail'; BUFFER='git swf'; _sc_suggest; print -r -- "[$REPLY]""#,
    );
    assert_eq!(
        added.lines().collect::<Vec<_>>(),
        ["[]", "[git switch main]", "[]"],
        "{added}"
    );
    let _ = fs::remove_dir_all(&root);
}

#[test]
fn zsh_picks_up_commands_typed_in_another_terminal() {
    if Command::new("zsh").arg("--version").output().is_err() {
        return;
    }
    let root = std::env::temp_dir().join(format!("sc-xtab-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("mkdir");
    assert!(sc(&root, &["init"]).0);
    assert!(sc(&root, &["apply", "--shell", "zsh"]).0);
    assert!(
        sc(
            &root,
            &[
                "hist",
                "add",
                "--cwd",
                "/work",
                "--session",
                "1",
                "--",
                "git status"
            ]
        )
        .0
    );
    // the reload that normally runs in the background is replaced by a synchronous one that counts
    let out = zsh(
        &root,
        r#"
_sc_hist_reload_async() { _t_reloads=$(( ${_t_reloads:-0} + 1 )); _sc_hist_apply "$(command shellcraft hist recent -n 3000)"; }
BUFFER='git s'; CURSOR=${#BUFFER}; _sc_suggest; print -r -- "first: [$REPLY] ready=$_sc_hist_ready"
_sc_cross_check; print -r -- "idle: reloads=${_t_reloads:-0}"
command shellcraft hist add --cwd /work --session 999999 -- 'git switch other'
BUFFER='git sw'; _sc_suggest; print -r -- "before check: [$REPLY]"
_sc_cross_check; _sc_suggest; print -r -- "other tab: [$REPLY] reloads=${_t_reloads:-0}"
_sc_cross_check; print -r -- "again: reloads=${_t_reloads:-0}"
_sc_cache_add 0 /work 'git stash mine'
command shellcraft hist add --cwd /work --session $$ -- 'git stash mine'
_sc_cross_check; print -r -- "own write: reloads=${_t_reloads:-0}"
command shellcraft hist add --cwd /work --session 999999 -- 'git show other'
_sc_cross_check; BUFFER='git st'; _sc_suggest; print -r -- "own kept after reload: [$REPLY] reloads=${_t_reloads:-0}"
"#,
    );
    let lines: Vec<&str> = out.lines().collect();
    assert_eq!(
        lines,
        [
            "first: [git status] ready=1",
            "idle: reloads=0",
            "before check: []",
            "other tab: [git switch other] reloads=1",
            "again: reloads=1",
            "own write: reloads=1",
            "own kept after reload: [git stash mine] reloads=2",
        ],
        "{out}"
    );
    let _ = fs::remove_dir_all(&root);
}
