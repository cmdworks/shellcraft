use std::process::{Command, Output};

fn run(data: &std::path::Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_shellcraft"))
        .env("XDG_DATA_HOME", data)
        .env("HOME", data)
        .args(args)
        .output()
        .expect("spawn shellcraft")
}

fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

#[test]
fn add_search_list_stats() {
    let data = std::env::temp_dir().join(format!("sc-hist-cli-{}", std::process::id()));
    let add = |exit: &str, cwd: &str, cmd: &[&str]| {
        let mut a = vec![
            "hist",
            "add",
            "--exit",
            exit,
            "--cwd",
            cwd,
            "--duration",
            "12",
            "--",
        ];
        a.extend_from_slice(cmd);
        assert!(run(&data, &a).status.success());
    };
    add("0", "/a", &["git", "status"]);
    add("1", "/a", &["git", "push", "--force"]);
    add("0", "/b", &["git", "status"]);
    add("0", "/a", &[" secret"]);
    add("0", "/a", &["echo", "multi\nline"]);

    assert_eq!(
        out(&run(&data, &["hist", "search", "git"])),
        "git status\ngit push --force\n"
    );
    assert_eq!(
        out(&run(&data, &["hist", "search", "git", "--all"]))
            .lines()
            .count(),
        3
    );
    assert_eq!(
        out(&run(&data, &["hist", "search", "git", "--ok"])),
        "git status\n"
    );
    assert_eq!(
        out(&run(&data, &["hist", "search", "-n", "1"])),
        "echo multi line\n"
    );
    assert!(!out(&run(&data, &["hist", "search", "secret"])).contains("secret"));

    let list = out(&run(&data, &["hist", "list", "push"]));
    assert!(list.contains("/a") && list.contains("12ms"), "{list}");
    let stats = out(&run(&data, &["hist", "stats"]));
    assert!(stats.starts_with("commands: 4  failed: 1"), "{stats}");
    assert!(stats.contains("2  git status"), "{stats}");
    let _ = std::fs::remove_dir_all(&data);
}
