"""Exercise the actual application in a PTY, with no external requests or changes.

Usage: python3 tests/terminal_smoke.py target/release/nexus
"""
import fcntl
import os
import pty
import select
import struct
import subprocess
import sys
import tempfile
import time
import termios

master, slave = pty.openpty()
fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 36, 130, 0, 0))
before = termios.tcgetattr(slave)
binary = os.path.abspath(sys.argv[1])
with tempfile.TemporaryDirectory(prefix="nexus-pty-") as data:
    env = dict(os.environ, TERM="xterm-256color", XDG_CONFIG_HOME=data, XDG_DATA_HOME=data)
    process = subprocess.Popen([binary, "--fresh"], stdin=slave, stdout=slave, stderr=slave, env=env)
    output = bytearray()

    def drain(seconds):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            ready, _, _ = select.select([master], [], [], min(0.1, deadline - time.monotonic()))
            if ready:
                try:
                    output.extend(os.read(master, 65536))
                except OSError:
                    break

    def send(keys, pause=0.3):
        os.write(master, keys)
        drain(pause)

    try:
        drain(1.2)
        send(b"2")              # Interfaces page
        send(b".")              # Secondary navigation: Wi-Fi
        send(b",")              # Back to interfaces
        send(b"\r")            # Details overlay
        send(b"\x1b")          # Close details
        send(b"\x0b")          # Ctrl+K palette
        send(b"Set interface MTU")
        send(b"\r")            # Open form only
        send(b"\x1b")          # Cancel; no configuration is submitted
        send(b"4")              # DNS
        send(b"a")              # Page-specific lookup form
        send(b"\x1b")          # Cancel; no query is sent
        send(b"2")
        send(b"t")              # Theme switch
        send(b"r", 0.5)        # Local refresh; page should remain Interfaces
        send(b"?")              # Help overlay
        send(b"\x1b")
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0))
        drain(0.3)
        send(b"1")              # Compact overview
        for height, width in [(48,60),(12,40),(48,200),(24,80)]:
            fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH",height,width,0,0))
            drain(0.15)
            send(b"\t",0.1)
        send(b"\x0b")
        send(b"Connect to Pi-hole v6")
        send(b"\r")
        send(b"\x1b") # Masked credential form cancelled; no requests.
        send(b"q")
        process.wait(timeout=5)
        drain(0.1)
        after = termios.tcgetattr(slave)
        text = output.decode("utf-8", "replace")
        assert process.returncode == 0, text[-3000:]
        for expected in ["NEXUS", "Interfaces", "COMMAND PALETTE", "Set runtime MTU", "HELP", "Connect to Pi-hole v6"]:
            assert expected in text, f"Missing rendered surface: {expected}"
        assert "\x1b[?1049h" in text and "\x1b[?1049l" in text, "Alternate screen not restored"
        assert after[3] & (termios.ICANON | termios.ECHO) == before[3] & (termios.ICANON | termios.ECHO), "Raw mode not restored"
        assert not os.listdir(data), "--fresh must not overwrite persistent settings/history"
        print("PTY smoke passed: navigation, details, command palette, cancelled MTU form, theme, refresh, resize, help, clean exit and terminal restoration")
    finally:
        if process.poll() is None:
            process.kill()
            process.wait()
os.close(master)
os.close(slave)
