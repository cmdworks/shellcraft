use std::fs;
use std::process::{Command, Output};

fn run(data: &std::path::Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_shellcraft"))
        .env("XDG_DATA_HOME", data)
        .env("HOME", data)
        .args(args)
        .output()
        .expect("spawn shellcraft")
}

#[test]
fn add_query_list_remove_prune() {
    let root = std::env::temp_dir().join(format!("sc-jump-{}", std::process::id()));
    let data = root.join("data");
    let a = root.join("proj").join("alpha");
    let b = root.join("proj").join("beta");
    fs::create_dir_all(&a).expect("mkdir a");
    fs::create_dir_all(&b).expect("mkdir b");
    let (a_s, b_s) = (a.to_str().expect("utf8"), b.to_str().expect("utf8"));

    assert!(run(&data, &["jump", "add", a_s]).status.success());
    assert!(run(&data, &["jump", "add", b_s]).status.success());
    assert!(run(&data, &["jump", "add", b_s]).status.success());

    let q = run(&data, &["jump", "query", "a"]);
    let best = String::from_utf8_lossy(&q.stdout).trim().to_string();
    assert!(best.ends_with("beta"), "most visited wins: {best}");

    let q = run(&data, &["jump", "query", "alp"]);
    assert!(String::from_utf8_lossy(&q.stdout).trim().ends_with("alpha"));
    assert!(!run(&data, &["jump", "query", "nothing"]).status.success());

    let list = run(&data, &["jump", "list"]);
    assert_eq!(String::from_utf8_lossy(&list.stdout).lines().count(), 2);

    fs::remove_dir_all(&a).expect("rm a");
    assert!(
        !run(&data, &["jump", "query", "alp"]).status.success(),
        "deleted dirs are not returned"
    );
    let pr = run(&data, &["jump", "prune"]);
    assert!(String::from_utf8_lossy(&pr.stdout).contains("removed 1"));
    assert!(run(&data, &["jump", "remove", b_s]).status.success());
    assert!(!run(&data, &["jump", "remove", b_s]).status.success());
    let _ = fs::remove_dir_all(&root);
}
