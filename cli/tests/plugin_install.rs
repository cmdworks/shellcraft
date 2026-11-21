use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

struct Env {
    root: PathBuf,
}

impl Env {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("sc-pinst-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("cfg/shellcraft")).expect("mkdir");
        let e = Self { root };
        assert!(e.run(&["init"], "").0);
        e
    }

    fn run(&self, args: &[&str], stdin: &str) -> (bool, String) {
        let mut child = Command::new(env!("CARGO_BIN_EXE_shellcraft"))
            .env("HOME", &self.root)
            .env("XDG_CONFIG_HOME", self.root.join("cfg"))
            .env("XDG_CACHE_HOME", self.root.join("cache"))
            .env("XDG_DATA_HOME", self.root.join("data"))
            .env("SHELLCRAFT_NO_BG", "1")
            .env("SHELL", "/bin/zsh")
            .env_remove("SHELLCRAFT_CONFIG")
            .current_dir(&self.root)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn");
        child
            .stdin
            .take()
            .expect("stdin")
            .write_all(stdin.as_bytes())
            .expect("stdin write");
        let out = child.wait_with_output().expect("wait");
        (
            out.status.success(),
            String::from_utf8_lossy(&out.stdout).into_owned()
                + &String::from_utf8_lossy(&out.stderr),
        )
    }

    fn init_zsh(&self) -> String {
        fs::read_to_string(self.root.join("cache/shellcraft/init.zsh")).expect("init.zsh")
    }

    fn lock(&self) -> String {
        fs::read_to_string(self.root.join("cfg/shellcraft/shellcraft.lock")).unwrap_or_default()
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn write_plugin(dir: &Path, version: &str, network: bool) {
    fs::create_dir_all(dir.join("bin")).expect("mkdir");
    fs::create_dir_all(dir.join("snippets")).expect("mkdir");
    fs::write(
        dir.join("shellcraft-plugin.toml"),
        format!(
            "name = \"demo\"\nversion = \"{version}\"\ndescription = \"demo plugin\"\nlicense = \"MIT\"\n\n[segment.main]\ncommand = \"bin/seg\"\nwhen = {{ files = [\"go.mod\"] }}\n\n[snippets]\nzsh = \"snippets/demo.zsh\"\n\n[aliases]\ndemoalias = \"echo demo\"\n\n[permissions]\nexec = [\"echo\"]\nnetwork = {network}\n"
        ),
    )
    .expect("manifest");
    fs::write(
        dir.join("snippets/demo.zsh"),
        format!("demo_hello() {{ echo hello-{version}; }}\n"),
    )
    .expect("snippet");
    fs::write(dir.join("bin/seg"), "#!/bin/sh\necho \"[tool] demo-seg\"\n").expect("seg");
    fs::set_permissions(dir.join("bin/seg"), fs::Permissions::from_mode(0o755)).expect("chmod");
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
            "-C",
        ])
        .arg(dir)
        .args(args)
        .output()
        .expect("git");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[test]
fn local_install_lock_tamper_and_remove() {
    let e = Env::new("local");
    let src = e.root.join("src/demo");
    write_plugin(&src, "0.1.0", false);
    let src_s = src.to_str().expect("utf8");

    // declining installs nothing
    let (ok, out) = e.run(&["plugin", "add", src_s], "n\n");
    assert!(!ok && out.contains("asks for: network: no"), "{out}");
    assert!(!e.root.join("data/shellcraft/plugins/demo").exists());

    let (ok, out) = e.run(&["plugin", "add", src_s, "--yes"], "");
    assert!(ok, "{out}");
    assert!(
        out.contains("snippet:  zsh") && out.contains("adds:     1 aliases"),
        "{out}"
    );
    let (ok, again) = e.run(&["plugin", "add", src_s, "--yes"], "");
    assert!(!ok && again.contains("already installed"), "{again}");

    let init = e.init_zsh();
    assert!(
        init.contains("demo_hello()"),
        "snippet is in the init script"
    );
    assert!(init.contains("alias demoalias="), "plugin data is merged");
    let lock = e.lock();
    assert!(
        lock.contains("name = \"demo\"")
            && lock.contains("rev = \"local\"")
            && lock.contains("network: no"),
        "{lock}"
    );
    let baked = fs::read_to_string(e.root.join("cache/shellcraft/plugins.tsv")).expect("baked");
    assert!(
        baked.starts_with("demo\tmain\t") && baked.trim_end().ends_with("go.mod"),
        "{baked}"
    );
    let (_, list) = e.run(&["plugin", "list"], "");
    assert!(
        list.lines()
            .any(|l| l.starts_with("demo") && l.contains("installed")),
        "{list}"
    );

    // a user-defined alias beats the plugin's
    let cfg = e.root.join("cfg/shellcraft/shellcraft.toml");
    fs::write(&cfg, "[aliases]\ndemoalias = \"mine\"\n").expect("cfg write");
    assert!(e.run(&["apply", "--shell", "zsh"], "").0);
    assert!(e.init_zsh().contains("alias demoalias='mine'"));

    // editing an installed file breaks the lock: not loaded, warned about
    fs::write(
        e.root
            .join("data/shellcraft/plugins/demo/snippets/demo.zsh"),
        "evil() { :; }\n",
    )
    .expect("tamper");
    let (ok, out) = e.run(&["apply", "--shell", "zsh"], "");
    assert!(
        ok && out.contains("files changed since they were locked"),
        "{out}"
    );
    assert!(
        !e.init_zsh().contains("evil()"),
        "tampered snippet is not loaded"
    );
    assert_eq!(
        fs::read_to_string(e.root.join("cache/shellcraft/plugins.tsv")).expect("baked"),
        ""
    );

    let (ok, out) = e.run(&["plugin", "remove", "demo"], "");
    assert!(ok, "{out}");
    assert!(!e.root.join("data/shellcraft/plugins/demo").exists() && !e.lock().contains("demo"));
    assert!(!e.run(&["plugin", "remove", "demo"], "").0);
}

#[test]
fn git_install_and_update_asks_about_changed_permissions() {
    let e = Env::new("git");
    let repo = e.root.join("repo");
    write_plugin(&repo, "0.1.0", false);
    git(&repo, &["init", "-q", "."]);
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "v1"]);
    let first = git(&repo, &["rev-parse", "HEAD"]);
    let url = format!("file://{}", repo.display());

    let (ok, out) = e.run(&["plugin", "add", &url, "--yes"], "");
    assert!(ok, "{out}");
    assert!(
        e.lock().contains(&first),
        "lock records the commit: {}",
        e.lock()
    );
    assert!(e
        .run(&["plugin", "update", "--yes"], "")
        .1
        .contains("demo: up to date"));

    // v2 asks for network access
    write_plugin(&repo, "0.2.0", true);
    git(&repo, &["commit", "-q", "-am", "v2"]);
    let second = git(&repo, &["rev-parse", "HEAD"]);
    // plugins run as you and permissions are information: `update --yes` applies the new version
    // and shows what it now declares
    let (ok, out) = e.run(&["plugin", "update", "--yes"], "");
    assert!(
        ok && out.contains("0.1.0 -> 0.2.0")
            && out.contains("permissions it declares changed")
            && out.contains("network: yes"),
        "{out}"
    );
    assert!(e.lock().contains(&second) && e.lock().contains("network: yes"));
    assert!(
        e.init_zsh().contains("hello-0.2.0"),
        "new snippet is applied"
    );
    assert!(e
        .run(&["plugin", "update", "demo"], "")
        .1
        .contains("up to date"));
}

#[test]
fn rejects_bad_sources_and_manifests() {
    let e = Env::new("bad");
    let (ok, out) = e.run(&["plugin", "add", "not a source", "--yes"], "");
    assert!(!ok && out.contains("not a directory"), "{out}");
    let (ok, out) = e.run(&["plugin", "add", "ext::sh -c evil", "--yes"], "");
    assert!(!ok && out.contains("not a directory"), "{out}");
    let bad = e.root.join("src/bad");
    fs::create_dir_all(&bad).expect("mkdir");
    fs::write(
        bad.join("shellcraft-plugin.toml"),
        "name = \"bad\"\nversion = \"1\"\n[segment.a]\ncommand = \"/bin/sh\"\n",
    )
    .expect("manifest");
    let (ok, out) = e.run(&["plugin", "add", bad.to_str().expect("utf8"), "--yes"], "");
    assert!(!ok && out.contains("relative path"), "{out}");
    assert!(!e.root.join("data/shellcraft/plugins/bad").exists());
}

#[test]
fn new_then_check_and_install() {
    let e = Env::new("new");
    for kind in ["segment", "snippet", "data"] {
        let name = format!("mine-{kind}");
        let (ok, out) = e.run(&["plugin", "new", &name, "--kind", kind], "");
        assert!(ok, "{out}");
        let (ok, out) = e.run(&["plugin", "check", &name], "");
        assert!(ok, "{kind}: {out}");
        let (ok, out) = e.run(&["plugin", "add", &name, "--yes"], "");
        assert!(ok, "{kind}: {out}");
    }
    assert!(e
        .run(&["plugin", "new", "mine-segment"], "")
        .1
        .contains("already exists"));
    assert!(e.init_zsh().contains("mine-snippet-hello"));
    let (ok, out) = e.run(&["plugin", "check", "mine-segment"], "");
    assert!(
        ok && out.contains("segment main:") && !out.contains("demo"),
        "{out}"
    );
}

#[test]
fn command_and_hook_plugins() {
    let e = Env::new("cmdhook");
    let src = e.root.join("src/greeter");
    fs::create_dir_all(src.join("bin")).expect("mkdir");
    fs::write(
        src.join("shellcraft-plugin.toml"),
        "name = \"greeter\"\nversion = \"1.0.0\"\n\n[command.greet]\nrun = \"bin/greet\"\nabout = \"say hello\"\n\n[hook.cd]\nevent = \"cd\"\nrun = \"bin/oncd\"\n\n[hook.after]\nevent = \"post\"\nrun = \"bin/after\"\n",
    )
    .expect("manifest");
    let log = e.root.join("hook.log");
    for (file, body) in [
        ("greet", "#!/bin/sh\necho \"hello $1 from greeter\"\nexit 3\n".to_string()),
        ("oncd", format!("#!/bin/sh\necho \"$SHELLCRAFT_EVENT $SHELLCRAFT_CWD old=$SHELLCRAFT_OLDPWD\" >> {}\n", log.display())),
        ("after", format!("#!/bin/sh\necho \"$SHELLCRAFT_EVENT exit=$SHELLCRAFT_EXIT ms=$SHELLCRAFT_DURATION_MS cmd=$SHELLCRAFT_COMMAND\" >> {}\n", log.display())),
    ] {
        fs::write(src.join("bin").join(file), body).expect("script");
        fs::set_permissions(src.join("bin").join(file), fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    let (ok, out) = e.run(&["plugin", "add", src.to_str().expect("utf8"), "--yes"], "");
    assert!(ok, "{out}");
    assert!(
        out.contains("command:  shellcraft greet")
            && out.contains("hook:     cd runs bin/oncd on every cd event"),
        "{out}"
    );

    // the command runs and its exit status is passed through
    let (ok, out) = e.run(&["greet", "world"], "");
    assert!(
        !ok && out.contains("hello world from greeter"),
        "exit status 3 is kept: {out}"
    );
    let (ok, out) = e.run(&["gret"], "");
    assert!(!ok && out.contains("did you mean `greet`"), "{out}");
    let (_, list) = e.run(&["plugin", "list"], "");
    assert!(
        list.contains("command: shellcraft greet") && list.contains("hook: cd"),
        "{list}"
    );

    // the init script registers exactly the events some plugin listens to
    let init = e.init_zsh();
    assert!(
        init.contains("shellcraft hook cd") && init.contains("shellcraft hook post"),
        "{init}"
    );

    // hooks run with the event data in their environment (they are detached: wait for the log)
    let wait = |needle: &str| {
        for _ in 0..100 {
            if fs::read_to_string(&log)
                .unwrap_or_default()
                .contains(needle)
            {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(30));
        }
        false
    };
    assert!(e.run(&["hook", "cd", "--old", "/before"], "").0);
    assert!(wait("old=/before"), "{:?}", fs::read_to_string(&log));
    assert!(
        e.run(
            &[
                "hook",
                "post",
                "--exit",
                "2",
                "--duration",
                "150",
                "--cmd",
                "make test"
            ],
            ""
        )
        .0
    );
    assert!(
        wait("post exit=2 ms=150 cmd=make test"),
        "{:?}",
        fs::read_to_string(&log)
    );

    // another plugin cannot take the same command, nor a built-in name
    let other = e.root.join("src/clash");
    fs::create_dir_all(other.join("bin")).expect("mkdir");
    fs::write(other.join("bin/x"), "#!/bin/sh\n").expect("script");
    fs::write(
        other.join("shellcraft-plugin.toml"),
        "name = \"clash\"\nversion = \"1\"\n[command.greet]\nrun = \"bin/x\"\n",
    )
    .expect("manifest");
    let (ok, out) = e.run(
        &["plugin", "add", other.to_str().expect("utf8"), "--yes"],
        "",
    );
    assert!(
        !ok && out.contains("already provided by plugin greeter"),
        "{out}"
    );
    fs::write(
        other.join("shellcraft-plugin.toml"),
        "name = \"clash\"\nversion = \"1\"\n[command.theme]\nrun = \"bin/x\"\n",
    )
    .expect("manifest");
    let (ok, out) = e.run(
        &["plugin", "add", other.to_str().expect("utf8"), "--yes"],
        "",
    );
    assert!(!ok && out.contains("built-in command"), "{out}");

    // removing the plugin removes the command and the hook registrations
    assert!(e.run(&["plugin", "remove", "greeter"], "").0);
    assert!(!e.run(&["greet"], "").0);
    assert!(!e.init_zsh().contains("shellcraft hook"));
}

#[test]
fn new_command_and_hook_kinds_pass_check() {
    let e = Env::new("newkinds");
    for kind in ["command", "hook"] {
        let name = format!("k-{kind}");
        assert!(e.run(&["plugin", "new", &name, "--kind", kind], "").0);
        let (ok, out) = e.run(&["plugin", "check", &name], "");
        assert!(ok && out.contains("is executable"), "{kind}: {out}");
    }
}

#[test]
fn linked_plugins_follow_edits_and_removal_keeps_the_folder() {
    let e = Env::new("link");
    let src = e.root.join("dev/demo");
    write_plugin(&src, "0.1.0", false);
    let src_s = src.to_str().expect("utf8");

    let (ok, out) = e.run(&["plugin", "link", src_s], "");
    assert!(ok && out.contains("development mode"), "{out}");
    assert!(e.init_zsh().contains("hello-0.1.0"));
    let (_, list) = e.run(&["plugin", "list"], "");
    assert!(list.contains("LINKED (dev)"), "{list}");
    assert!(e.lock().contains("dev = true"), "{}", e.lock());

    // edit the snippet in place: no tamper warning, the next apply picks it up
    fs::write(
        src.join("snippets/demo.zsh"),
        "demo_hello() { echo hello-edited; }\n",
    )
    .expect("edit");
    let (ok, out) = e.run(&["apply", "--shell", "zsh"], "");
    assert!(ok && !out.contains("changed since"), "{out}");
    assert!(e.init_zsh().contains("hello-edited"));

    assert!(e
        .run(&["plugin", "update"], "")
        .1
        .contains("linked for development"));
    let (ok, out) = e.run(&["plugin", "link", src_s], "");
    assert!(!ok && out.contains("already installed"), "{out}");

    // removing a linked plugin removes the link, never the author's folder
    assert!(e.run(&["plugin", "remove", "demo"], "").0);
    assert!(src.join("shellcraft-plugin.toml").exists() && src.join("bin/seg").exists());
    assert!(!e.root.join("data/shellcraft/plugins/demo").exists());
    assert!(!e.init_zsh().contains("hello-edited"));
}

#[test]
fn newer_plugin_api_is_refused_with_a_clear_message() {
    let e = Env::new("api");
    let src = e.root.join("src/future");
    fs::create_dir_all(&src).expect("mkdir");
    fs::write(
        src.join("shellcraft-plugin.toml"),
        "api = 2\nname = \"future\"\nversion = \"1\"\n",
    )
    .expect("manifest");
    for args in [["plugin", "add"], ["plugin", "link"]] {
        let mut a = args.to_vec();
        a.push(src.to_str().expect("utf8"));
        a.push("--yes");
        let (ok, out) = e.run(&a[..if args[1] == "link" { 3 } else { 4 }], "");
        assert!(
            !ok && out.contains("plugin API 2") && out.contains("supports up to 1"),
            "{out}"
        );
    }
    let (ok, out) = e.run(&["plugin", "new", "fresh"], "");
    assert!(ok, "{out}");
    let manifest =
        fs::read_to_string(e.root.join("fresh/shellcraft-plugin.toml")).expect("manifest");
    assert!(manifest.starts_with("api = 1\n"), "{manifest}");
}

fn write_slot_plugin(dir: &Path, name: &str, extra: &str) {
    fs::create_dir_all(dir.join("slots")).expect("mkdir");
    fs::write(dir.join("slots/pre.zsh"), "echo SLOT_BEFORE_ALIASES\n").expect("slot file");
    fs::write(dir.join("slots/post.zsh"), "echo SLOT_AFTER_JUMP\n").expect("slot file");
    fs::write(
        dir.join("shellcraft-plugin.toml"),
        format!(
            "name = \"{name}\"\nversion = \"1.0.0\"\n{extra}\n[[slot]]\nshell = \"zsh\"\nname = \"aliases\"\nfile = \"slots/pre.zsh\"\nposition = \"before\"\n\n[[slot]]\nshell = \"zsh\"\nname = \"jump\"\nfile = \"slots/post.zsh\"\n\n[[slot]]\nshell = \"zsh\"\nname = \"nonsense\"\nfile = \"slots/post.zsh\"\n"
        ),
    )
    .expect("manifest");
}

#[test]
fn slots_requirements_and_conflicts() {
    let e = Env::new("slots");
    let (ok, out) = e.run(&["plugin", "slots", "zsh"], "");
    assert!(
        ok && out.lines().any(|l| l == "aliases") && out.lines().any(|l| l == "history"),
        "{out}"
    );
    assert!(!e.run(&["plugin", "slots", "csh"], "").0);

    let src = e.root.join("src/slotty");
    write_slot_plugin(&src, "slotty", "");
    let (ok, out) = e.run(&["plugin", "add", src.to_str().expect("utf8"), "--yes"], "");
    assert!(ok, "{out}");
    assert!(
        out.contains("no slot `nonsense`")
            || e.run(&["apply", "--shell", "zsh"], "")
                .1
                .contains("no slot `nonsense`")
    );
    let init = e.init_zsh();
    let at = |n: &str| init.find(n).unwrap_or_else(|| panic!("missing {n}"));
    assert!(
        at("echo SLOT_BEFORE_ALIASES") < at("# --- aliases"),
        "before the aliases slot"
    );
    assert!(
        at("# --- jump") < at("echo SLOT_AFTER_JUMP")
            && at("echo SLOT_AFTER_JUMP") < at("# --- command not found")
    );
    assert!(e.run(&["plugin", "remove", "slotty"], "").0);
    assert!(!e.init_zsh().contains("SLOT_BEFORE_ALIASES"));

    // requirements: unmet, then met by an installed plugin, a version bound, and a bundled plugin
    let needy = e.root.join("src/needy");
    write_slot_plugin(
        &needy,
        "needy",
        "requires = [\"corelib>=2.0\", \"git\"]\nconflicts = [\"enemy\"]\n",
    );
    let n = needy.to_str().expect("utf8");
    let (ok, out) = e.run(&["plugin", "add", n, "--yes"], "");
    assert!(
        !ok && out.contains("requires `corelib>=2.0`") && !out.contains("requires `git`"),
        "{out}"
    );
    let base = e.root.join("src/base");
    fs::create_dir_all(&base).expect("mkdir");
    fs::write(
        base.join("shellcraft-plugin.toml"),
        "name = \"corelib\"\nversion = \"1.9\"\n",
    )
    .expect("manifest");
    assert!(
        e.run(
            &["plugin", "add", base.to_str().expect("utf8"), "--yes"],
            ""
        )
        .0
    );
    let (ok, out) = e.run(&["plugin", "add", n, "--yes"], "");
    assert!(
        !ok && out.contains("requires `corelib>=2.0`"),
        "1.9 is below 2.0: {out}"
    );
    fs::write(
        base.join("shellcraft-plugin.toml"),
        "name = \"corelib\"\nversion = \"2.1\"\n",
    )
    .expect("manifest");
    assert!(e.run(&["plugin", "update", "--yes"], "").0);
    let (ok, out) = e.run(&["plugin", "add", n, "--yes"], "");
    assert!(ok, "{out}");
    let enemy = e.root.join("src/enemy");
    fs::create_dir_all(&enemy).expect("mkdir");
    fs::write(
        enemy.join("shellcraft-plugin.toml"),
        "name = \"enemy\"\nversion = \"1\"\n",
    )
    .expect("manifest");
    assert!(
        e.run(
            &["plugin", "add", enemy.to_str().expect("utf8"), "--yes"],
            ""
        )
        .0,
        "installing the conflicting plugin itself is fine"
    );
    let other = e.root.join("src/other");
    write_slot_plugin(&other, "other", "conflicts = [\"enemy\"]\n");
    let (ok, out) = e.run(
        &["plugin", "add", other.to_str().expect("utf8"), "--yes"],
        "",
    );
    assert!(!ok && out.contains("conflicts with `enemy`"), "{out}");
}
