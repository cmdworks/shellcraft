#!/usr/bin/env python3
"""Compare zsh with shellcraft against zsh with Oh My Zsh on the same machine.

    python3 scripts/bench/compare-omz.py --omz DIR [--runs 40]

DIR holds `ohmyzsh/` and `custom/plugins/{zsh-autosuggestions,zsh-syntax-highlighting}/` (plain
`git clone --depth 1` of the upstream repositories; nothing is installed or run from them except
by zsh itself). Uses the release binary (`cargo build --release` first). HOME, ZDOTDIR and the XDG
directories are temporary, so your own setup is not read.

Configurations (all start `zsh -i`):
  empty      empty .zshrc                                   the shell itself
  omz-min    Oh My Zsh, no plugins, default theme            what OMZ costs by itself
  omz-feat   Oh My Zsh + git, z, autosuggestions, syntax highlighting   comparable features
  shellcraft default config (built-in prompt with git, jump, history, suggestions, highlighting)

Measured: startup (`zsh -i -c exit`), Enter-to-prompt-ready latency (all precmd hooks, measured in
the line editor) in a git checkout and in a plain directory, key-to-echo latency while typing with suggestions and
highlighting active, and the first Tab press (OMZ runs compinit at startup, shellcraft on first Tab).
Runs are interleaved between configurations so drift hits all of them equally.
"""
import argparse, fcntl, os, pty, select, shutil, statistics, struct, subprocess, tempfile, termios, time
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
HISTORY = ["git status --short", "git stash", "git switch main", "git commit -m wip", "ls -la", "cargo build --release"]


def stats(xs):
    xs = sorted(xs)
    return f"median {statistics.median(xs):7.2f}  min {xs[0]:7.2f}  p90 {xs[int(len(xs) * 0.9) - 1]:7.2f}"


def make_configs(tmp: Path, omz: Path, binary: Path):
    env_base = dict(os.environ)
    for k in ("ZDOTDIR", "SHELLCRAFT_CONFIG", "STARSHIP_CONFIG"):
        env_base.pop(k, None)
    home = tmp / "home"; home.mkdir()
    env_base.update(HOME=str(home), TERM="xterm-256color", PATH=f"{binary.parent}:{env_base['PATH']}",
                    XDG_CONFIG_HOME=str(tmp / "xc"), XDG_DATA_HOME=str(tmp / "xd"), XDG_CACHE_HOME=str(tmp / "xk"))
    hist = tmp / "zsh_history"
    hist.write_text("".join(f": {1700000000 + i}:0;{c}\n" for i, c in enumerate(HISTORY)))
    cfgs = {}

    mark = r'''
zmodload zsh/datetime
__bench_mark() { print -rn -- $'\e]9999;'$EPOCHREALTIME$'\a'; }
precmd_functions+=(__bench_mark)
'''

    def cfg(name, rc):
        d = tmp / name; d.mkdir(); (d / ".zshrc").write_text(rc + mark)
        e = dict(env_base, ZDOTDIR=str(d)); cfgs[name] = e

    cfg("empty", "")
    # one completion dump per configuration: sharing it makes interleaved runs invalidate each other
    omz_head = lambda n: f'''export ZSH={omz / "ohmyzsh"}
export ZSH_CUSTOM={omz / "custom"}
export ZSH_COMPDUMP={tmp}/zcompdump-{n}
ZSH_THEME="robbyrussell"
DISABLE_AUTO_UPDATE="true"
zstyle ':omz:update' mode disabled
HISTFILE={hist}; HISTSIZE=50000; SAVEHIST=50000
'''
    cfg("omz-min", omz_head("min") + "plugins=()\nsource $ZSH/oh-my-zsh.sh\n")
    cfg("omz-feat", omz_head("feat") + "plugins=(git z zsh-autosuggestions zsh-syntax-highlighting)\nsource $ZSH/oh-my-zsh.sh\n")
    # shellcraft: default config; same history entries recorded in its own database
    e = dict(env_base)
    run = lambda *a: subprocess.run([str(binary), *a], env=e, check=True, capture_output=True)
    run("init"); run("apply", "--shell", "zsh")
    for c in HISTORY:
        run("hist", "add", "--cwd", str(REPO), "--", c)
    cfg("shellcraft", f"source {tmp}/xk/shellcraft/init.zsh\n")
    cfgs["shellcraft"].update(XDG_CONFIG_HOME=str(tmp / "xc"), XDG_DATA_HOME=str(tmp / "xd"), XDG_CACHE_HOME=str(tmp / "xk"))
    return cfgs


def run_zsh(env, code, cwd=None):
    t = time.perf_counter()
    r = subprocess.run(["zsh", "-i", "-c", code], env=env, cwd=cwd, stdin=subprocess.DEVNULL, capture_output=True)
    return (time.perf_counter() - t) * 1000, r


class Pty:
    def __init__(self, env, cwd):
        self.pid, self.fd = pty.fork()
        if self.pid == 0:
            fcntl.ioctl(0, termios.TIOCSWINSZ, struct.pack("HHHH", 40, 120, 0, 0))
            os.chdir(cwd); os.execvpe("zsh", ["zsh", "-i"], env)

    def drain(self, quiet=0.15, limit=6.0):
        end = time.time() + limit; last = time.time(); out = b""
        while time.time() < end and time.time() - last < quiet:
            r, _, _ = select.select([self.fd], [], [], 0.02)
            if r:
                try: d = os.read(self.fd, 65536)
                except OSError: break
                out += d; last = time.time()
        return out

    def key_latency(self, ch):
        os.write(self.fd, ch.encode()); t = time.perf_counter()
        r, _, _ = select.select([self.fd], [], [], 2.0)
        dt = (time.perf_counter() - t) * 1000 if r else float("nan")
        self.drain(0.05)
        return dt

    def close(self):
        try: os.write(self.fd, b"\x15exit\r"); time.sleep(0.1); os.kill(self.pid, 9)
        except OSError: pass
        try: os.waitpid(self.pid, 0)
        except OSError: pass


def prompt_latency(env, cwd, rounds=30, lines=(b"\r",)):
    """Enter on an empty line, two timings: `ready` = the last precmd hook has run (the prompt is
    drawn right after and you can type), `final` = the last byte of output arrived (a setup that
    redraws its prompt later, like shellcraft's async one, is only final then)."""
    import re
    p = Pty(env, cwd); p.drain(0.6, 15)
    ready, final = [], []
    for i in range(rounds):
        t = time.time(); os.write(p.fd, lines[i % len(lines)])
        data = b""; last = time.time()
        while time.time() - last < 0.25 and time.time() - t < 5:
            r, _, _ = select.select([p.fd], [], [], 0.01)
            if r:
                try: data += os.read(p.fd, 65536)
                except OSError: break
                last = time.time()
        m = re.search(rb"\x1b\]9999;([0-9.]+)\x07", data)
        if m:
            ready.append((float(m.group(1)) - t) * 1000)
            final.append((last - t) * 1000)
    p.close()
    return ready, final


def typing(env, cwd, rounds=12):
    p = Pty(env, cwd); p.drain(0.6, 15)
    lat = []
    for _ in range(rounds):
        for ch in "git sta":
            lat.append(p.key_latency(ch))
        os.write(p.fd, b"\x15"); p.drain(0.1)
    p.close()
    return [x for x in lat if x == x]


def first_tab(env, cwd, prompts_before=0):
    """Time from pressing Tab to the first output (the completion system may be loaded or
    initialised first), after typing `git sta`. `prompts_before` empty Enters come first."""
    p = Pty(env, cwd); p.drain(0.6, 15)
    for _ in range(prompts_before):
        os.write(p.fd, b"\r"); p.drain(0.4, 10)
    os.write(p.fd, b"git sta"); p.drain(0.2)
    t = time.perf_counter(); os.write(p.fd, b"\t")
    r, _, _ = select.select([p.fd], [], [], 10.0)
    dt = (time.perf_counter() - t) * 1000 if r else float("nan")
    p.drain(0.3, 8)
    p.close()
    return dt


def main():
    ap = argparse.ArgumentParser(); ap.add_argument("--omz", required=True, type=Path); ap.add_argument("--runs", type=int, default=40)
    a = ap.parse_args()
    binary = REPO / "target/release/shellcraft"
    if not binary.exists():
        raise SystemExit("build first: cargo build --release")
    tmp = Path(tempfile.mkdtemp(prefix="sc-omz-"))
    try:
        cfgs = make_configs(tmp, a.omz.resolve(), binary)
        names = list(cfgs)
        for n in names:  # warm-up: builds completion dumps and caches
            for _ in range(3): run_zsh(cfgs[n], "exit")
        res = {n: [] for n in names}
        for _ in range(a.runs):
            for n in names:
                res[n].append(run_zsh(cfgs[n], "exit")[0])
        print(f"== startup: zsh -i -c exit ({a.runs} runs, interleaved), ms")
        base = statistics.median(res["empty"])
        for n in names:
            print(f"{n:11} {stats(res[n])}   overhead vs empty {statistics.median(res[n]) - base:6.2f}")
        for label, cwd in (("git checkout (this repo)", str(REPO)), ("plain directory", str(tmp))):
            print(f"== Enter on an empty line, {label}, ms (30 rounds): ready = you can type, final = prompt fully drawn")
            for n in names:
                ready, final = prompt_latency(cfgs[n], cwd)
                print(f"{n:11} ready {stats(ready)}")
                print(f"{'':11} final {stats(final)}")
        print("== cd between the repo and a plain directory (the git segment changes), ms: ready / final")
        cds = (f"cd {REPO}\r".encode(), f"cd {tmp}\r".encode())
        for n in names:
            ready, final = prompt_latency(cfgs[n], str(tmp), 30, cds)
            print(f"{n:11} ready {stats(ready)}")
            print(f"{'':11} final {stats(final)}")
        print("== typing: key to echo latency with suggestion + highlighting, ms (zsh/pty, 84 keys)")
        for n in names:
            print(f"{n:11} {stats(typing(cfgs[n], str(REPO)))}")
        for before, label in ((0, "as the very first action"), (1, "after one command (second prompt)")):
            print(f"== first Tab press after typing `git sta`, {label}, ms")
            for n in names:
                v = [first_tab(cfgs[n], str(REPO), before) for _ in range(5)]
                print(f"{n:11} {stats(v)}")
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


if __name__ == "__main__":
    main()
