# pty smoke test of `shellcraft` with no arguments (the TUI). Run from the repo root after `cargo build`.
import pty,os,sys,time,select,tempfile
d=tempfile.mkdtemp()
env=dict(os.environ,XDG_CONFIG_HOME=d+"/c",XDG_DATA_HOME=d+"/d",XDG_CACHE_HOME=d+"/k",HOME=d,SHELL="/bin/zsh",TERM="xterm-256color")
pid,fd=pty.fork()
if pid==0:
    os.execve("target/debug/shellcraft",["shellcraft"],env)
def rd(t=1.0):
    out=b""
    end=time.time()+t
    while time.time()<end:
        r,_,_=select.select([fd],[],[],0.1)
        if r:
            try: out+=os.read(fd,65536)
            except OSError: break
    return out
import fcntl,termios,struct
fcntl.ioctl(fd,termios.TIOCSWINSZ,struct.pack("HHHH",24,80,0,0))
o=rd(1.5); print("welcome" , b"Nothing is written" in o)
os.write(fd,b"\r"); o=rd(); print("features", b"Built-in prompt" in o)
os.write(fd,b" "); os.write(fd,b"\x1b[C\x1b[C\x1b[C\x1b[C"); o=rd(); print("review", b"builtin = false" in o or b"builtin" in o)
os.write(fd,b"\r"); o=rd(3)
print("applied", os.path.exists(d+"/c/shellcraft/shellcraft.toml"))
print(open(d+"/c/shellcraft/shellcraft.toml").read()[:200] if os.path.exists(d+"/c/shellcraft/shellcraft.toml") else o[-300:])
