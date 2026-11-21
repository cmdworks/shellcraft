#!/usr/bin/env python3
"""Interactive sweep: drive a real shell in a pty and check shellcraft's features.

    python3 scripts/sweep/run.py zsh|bash|fish|nu|pwsh [--pwsh /path/to/pwsh]

Builds the debug binary, creates a sandbox (config, data, cache and fixtures in a temp dir, your
real shell setup is not touched), starts the shell with only shellcraft's init script loaded, types
into it and checks what appears. Prints PASS/FAIL per check; exit status 1 if any fails.
"""
import argparse, fcntl, os, pty, re, select, shutil, struct, subprocess, sys, tempfile, termios, time
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
# the prompt shows the branch, or the short commit on a detached HEAD (a CI checkout of a tag)
BRANCH = (subprocess.run(["git", "branch", "--show-current"], cwd=REPO, capture_output=True, text=True).stdout.strip()
          or subprocess.run(["git", "rev-parse", "--short=7", "HEAD"], cwd=REPO, capture_output=True, text=True).stdout.strip() or "HEAD")

def sh(*cmd, **kw):
    return subprocess.run(cmd, check=True, capture_output=True, text=True, **kw)

def setup(tmp: Path, shell: str):
    """Sandbox environment, fixtures, and the generated init script."""
    env = os.environ.copy()
    env.update(
        XDG_CONFIG_HOME=str(tmp / "config"), XDG_DATA_HOME=str(tmp / "data"), XDG_CACHE_HOME=str(tmp / "cache"),
        TERM="xterm-256color", SHELL={"zsh": "/bin/zsh", "bash": "/bin/bash"}.get(shell, "/bin/sh"),
        PATH=f"{REPO}/target/debug:{tmp}/fakebin:{env['PATH']}:/opt/homebrew/bin:/usr/local/bin:{Path.home()}/.cargo/bin", DOTNET_CLI_TELEMETRY_OPTOUT="1", POWERSHELL_TELEMETRY_OPTOUT="1",
    )
    env.pop("SHELLCRAFT_CONFIG", None)
    fx = tmp / "fx"
    for d in ("node", "dock", "home/sub", "fakebin"):
        (tmp / d if d == "fakebin" else fx / d).mkdir(parents=True, exist_ok=True)
    (fx / "node/.nvmrc").write_text("18\n"); (fx / "node/package.json").write_text("{}")
    (fx / "dock/Dockerfile").write_text("FROM x\n"); (fx / "home/file_sweep.txt").write_text("one\n")
    tool = tmp / "fakebin/mytool"
    tool.write_text("#!/bin/sh\ncase \"$1\" in\n  deploy) printf 'Options:\\n      --force   skip checks\\n      --env <E>  target\\n' ;;\n"
                    "  *) printf 'Commands:\\n  deploy   Ship the build\\n  destroy  Tear it down\\n  status   Show state\\n\\nOptions:\\n  -q, --quiet  Be quiet\\n' ;;\nesac\n")
    tool.chmod(0o755)
    run = lambda *a: subprocess.run(a, env=env, check=True, capture_output=True, text=True)
    run("shellcraft", "init")
    cfg = tmp / "config/shellcraft/shellcraft.toml"
    cfg.write_text(cfg.read_text().replace('# gco = "git checkout"', 'zz = "echo ABBR_OK"'))
    run("shellcraft", "apply", "--shell", shell)
    run("shellcraft", "hist", "add", "--cwd", str(REPO), "--", "echo", "HIST_SWEEP_OK")
    init = tmp / "cache/shellcraft" / ("init.nu" if shell == "nu" else "init.ps1" if shell == "pwsh" else f"init.{shell}")
    return env, fx, init

def launch(shell, env, init, tmp, pwsh):
    """Argv that starts `shell` with only the init script loaded."""
    if shell == "zsh":
        z = tmp / "zdot"; z.mkdir(exist_ok=True); (z / ".zshrc").write_text(f"source {init}\n"); env["ZDOTDIR"] = str(z)
        return ["zsh", "-i"]
    if shell == "bash":
        rc = tmp / "bashrc"; rc.write_text(f"source {init}\n"); return ["bash", "--rcfile", str(rc), "-i"]
    if shell == "fish":
        return ["fish", "-i", "--no-config", "-C", f"source {init}"]
    if shell == "nu":
        c, e = tmp / "config.nu", tmp / "env.nu"; c.write_text(f"source {init}\n"); e.write_text(""); return ["nu", "--config", str(c), "--env-config", str(e)]
    return [pwsh, "-NoProfile", "-NoExit", "-Command", f". {init}"]

class Term:
    def __init__(self, argv, env, cwd):
        self.pid, self.fd = pty.fork()
        if self.pid == 0:
            # window size first, so the shell sees 40x120 from its first instruction
            fcntl.ioctl(0, termios.TIOCSWINSZ, struct.pack("HHHH", 40, 120, 0, 0))
            os.chdir(cwd); os.execvpe(argv[0], argv, env)
    def read(self, secs):
        out = b""; end = time.time() + secs
        while time.time() < end:
            r, _, _ = select.select([self.fd], [], [], 0.05)
            if r:
                try: d = os.read(self.fd, 16384)
                except OSError: break
                for q, a in ((b"\x1b[6n", b"\x1b[1;1R"), (b"\x1b[c", b"\x1b[?62;c"), (b"\x1b[0c", b"\x1b[?62;c"), (b"\x1b[>0q", b"\x1bP>|xterm(388)\x1b\\"), (b"\x1b[?u", b"\x1b[?0u"), (b"\x1b]11;?", b"\x1b]11;rgb:0000/0000/0000\x1b\\"), (b"\x1bP+q", b"\x1bP0+r\x1b\\")):
                    if q in d: os.write(self.fd, a)
                out += d
        return re.sub(r"\x1b\[[0-9;?]*[A-Za-z]|\x1b[()][A-B0-9]|\x1b\][^\x07]*\x07|\r", "", out.decode(errors="replace"))
    def send(self, text, wait):
        self.read(0.2); os.write(self.fd, text.encode("latin-1")); return self.read(wait)

def steps(shell, fx):
    f = str(fx)
    common = [  # (name, keys, wait, expected substrings)
        ("prompt with branch and symbol", "\r", 2.0, [BRANCH, "❯"]),
        ("ll lists", "ll features\r", 2.0, ["drwx", "jump"]),
        ("cat plain when piped", "cat Cargo.toml | head -2\r", 1.5, ["[workspace]"]),
        ("cd then j", f"cd {f}/home/sub\r", 1.5, ["sub"]),
        ("cd away", "cd /\r", 1.0, ["❯"]),
        ("j jumps back", "j sub\r", 2.0, ["home/sub"]),
        ("versions: node", f"cd {f}/node\r", 2.0, ["node"]),
        ("versions: active vs declared", "\r", 3.5, ["wants 18"]),
        ("history recorded", "echo SWEEP_HIST_MARK\r", 1.5, ["SWEEP_HIST_MARK"]),
        ("ctrl-r picker", "\x12", 1.5, ["/"]),
        ("ctrl-r pick and run", "SWEEP_HIST_MARK\r\r", 2.0, ["SWEEP_HIST_MARK"]),
        ("go to files dir", f"\x15cd {f}/home\r", 1.5, ["home"]),
        ("ctrl-t picker", "echo \x14", 2.0, ["file_sweep"]),
        ("ctrl-t insert", "file_sweep\r", 1.5, ["file_sweep.txt"]),
    ]
    extra = {
        "zsh": [("abbreviation", "\x15zz \r", 1.5, ["ABBR_OK"]), ("not-found hint", "\x15gti status\r", 2.5, ["command not found: gti", "did you mean"]),
                ("completion from --help", "\x15mytool de\t", 2.5, ["deploy"]), ("completion flags", "\x15mytool deploy --\t", 2.5, ["--force"]),
                ("git subcommand completion", "\x15git sta\t", 4.0, ["status"]),   # a menu lists stash and status; a unique match completes to status
                ("option-c directory picker", "\x15\xc3\xa7", 1.5, ["/"])],
        "bash": [("exit status in prompt", "\x15false\r", 1.5, ["[1]"])],
        "fish": [("abbreviation", "\x15zz \r", 2.0, ["ABBR_OK"]), ("not-found hint", "\x15gti status\r", 3.0, ["command not found: gti", "did you mean"])],
        "nu": [("not-found hint", "\x15gti\r", 3.0, ["did you mean"])],
        "pwsh": [("exit status in prompt", "\x15cmd /c exit 3\r", 2.5, ["["])],
    }
    return common + extra[shell]

def main():
    ap = argparse.ArgumentParser(); ap.add_argument("shell", choices=["zsh", "bash", "fish", "nu", "pwsh"])
    ap.add_argument("--pwsh", default=shutil.which("pwsh")); a = ap.parse_args()
    if a.shell == "pwsh" and not a.pwsh: sys.exit("pwsh not found; pass --pwsh /path/to/pwsh")
    subprocess.run(["cargo", "build", "--quiet"], cwd=REPO, check=True)
    tmp = Path(tempfile.mkdtemp(prefix="sc-sweep-"))
    try:
        env, fx, init = setup(tmp, a.shell)
        t = Term(launch(a.shell, env, init, tmp, a.pwsh), env, REPO)
        t.read(8)
        results = []
        for name, keys, wait, expect in steps(a.shell, fx):
            out = t.send(keys, wait); ok = all(e in out for e in expect)
            results.append(ok); print(("PASS " if ok else "FAIL ") + name + ("" if ok else "   -> " + out[-160:].replace("\n", "|")))
        os.write(t.fd, b"\x15\x15exit\r"); time.sleep(0.5)
        print(f"\n{sum(results)}/{len(results)} passed"); sys.exit(0 if all(results) else 1)
    finally:
        shutil.rmtree(tmp, ignore_errors=True)

if __name__ == "__main__":
    main()
