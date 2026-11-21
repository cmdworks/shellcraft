//! The line that puts shellcraft's own folder on PATH parses in every shell and keeps odd
//! characters literal (each shell's syntax checker runs when that shell is installed).

use shellcraft_shell::{path_line, ShellKind};
use std::io::Write;
use std::process::{Command, Stdio};

fn check(shell: &str, args: &[&str], text: &str) {
    let Ok(mut child) = Command::new(shell)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    else {
        return; // shell not installed
    };
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(text.as_bytes())
        .expect("write");
    let out = child.wait_with_output().expect("wait");
    assert!(
        out.status.success(),
        "{shell} rejected:\n{text}\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn path_line_is_valid_and_literal() {
    let dir = "/home/me/it's here/bin";
    for (kind, shell, args) in [
        (ShellKind::Zsh, "zsh", vec!["-n", "/dev/stdin"]),
        (ShellKind::Bash, "bash", vec!["-n", "/dev/stdin"]),
        (ShellKind::Fish, "fish", vec!["-n", "/dev/stdin"]),
    ] {
        let line = path_line(kind, dir);
        assert!(line.contains("it"), "{line}");
        check(shell, &args, &line);
    }
    // run it for real in the POSIX shells: PATH gains the folder exactly once
    for shell in ["zsh", "bash"] {
        let line = path_line(ShellKind::Zsh, "/opt/sc test");
        let script = format!("PATH=/usr/bin:/bin\n{line}{line}\nprintf '%s\\n' \"$PATH\"");
        let Ok(out) = Command::new(shell).args(["-c", &script]).output() else {
            continue;
        };
        let got = String::from_utf8_lossy(&out.stdout).trim().to_string();
        assert_eq!(got, "/opt/sc test:/usr/bin:/bin", "{shell}");
    }
    assert!(path_line(ShellKind::Nushell, dir).starts_with("$env.PATH"));
    assert!(path_line(ShellKind::Pwsh, dir).contains("PathSeparator"));
}
