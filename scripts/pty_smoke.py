#!/usr/bin/env python3
"""Live smoke test: drives the real TUI in a pty (menu arrows, tab completion, hidden key entry, esc, quit).
Usage: scripts/pty_smoke.py target/release/portcode <empty temp dir>"""
import pty, os, sys, time, select, struct, fcntl, termios, re
exe, cwd = sys.argv[1], sys.argv[2]
pid, fd = pty.fork()
if pid == 0:
    os.chdir(cwd)
    os.environ.update(TERM="xterm-256color", PORTCODE_NO_UPDATE_CHECK="1", HOME=cwd)
    os.environ.pop("LAKOMOOR_API_KEY", None)
    os.execv(exe, [exe])
fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", 32, 110, 0, 0))
buf = b""
def pump(t=0.8):
    global buf
    end = time.time() + t
    while time.time() < end:
        r, _, _ = select.select([fd], [], [], 0.1)
        if r:
            try: buf += os.read(fd, 65536)
            except OSError: return
def send(s, t=0.6): os.write(fd, s.encode()); pump(t)
pump(1.5)
send("/prov"); send("\x1b[B"); send("\x1b[A")      # down, up in the menu
send("\t")                                          # complete
send("oll"); send("\t")                             # argument menu: ollama
send("\x1b[D\x1b[C")                                # left/right
send("\x15")                                        # ctrl+u
send("/key"); send("\r")                            # key prompt
send("sk-secret-123")
send("\x1b")                                        # esc cancels
try:
    send("\x03"); send("\x03", 1.0)
except OSError:
    pass
try:
    _, st = os.waitpid(pid, os.WNOHANG)
except ChildProcessError:
    st = 0
txt = re.sub(rb"\x1b\[[0-9;?]*[A-Za-z]", b" ", buf).decode("utf8", "replace")
print("secret leaked:", "sk-secret-123" in txt)
for w in ["No API key", "provider", "ollama", "needs a key", "Paste the API key", "cancelled"]:
    print(f"{w!r}:", w in txt)
print("exit status:", st)
