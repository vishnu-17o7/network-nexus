"""Exercise PNG transport, cache, freeze, modal cleanup, resize and text fallback in a real PTY.

This validates the application/protocol, not a particular emulator's compositor.
"""
import base64
import fcntl
import os
import pty
import re
import select
import struct
import subprocess
import sys
import tempfile
import termios
import time

binary = os.path.abspath(sys.argv[1])
APC = re.compile(rb'\x1b_G([^;]*);(.*?)\x1b\\', re.S)

def images(data):
    payload = bytearray()
    meta = None
    found = []
    for header, chunk in APC.findall(data):
        fields = dict(item.split(b'=', 1) for item in header.split(b',') if b'=' in item)
        if fields.get(b'a') == b'T':
            assert fields[b'C'] == b'1' and fields[b'q'] == b'2'
            meta = fields
            payload.clear()
        elif fields.get(b'a') == b'd':
            assert fields.get(b'd') == b'I', 'Never globally delete other applications images'
            continue
        if meta is None:
            continue
        assert len(chunk) <= 4096
        payload.extend(chunk)
        if fields.get(b'm', b'0') == b'0':
            png = base64.b64decode(payload, validate=True)
            assert png.startswith(b'\x89PNG\r\n\x1a\n')
            width, height = struct.unpack('>II', png[16:24])
            assert width == int(meta[b'c']) * 10 and height == int(meta[b'r']) * 23
            found.append((meta, png))
            meta = None
    return found

for mode in ['kitty', 'text']:
    master, slave = pty.openpty()
    fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', 42, 140, 1400, 966))
    before = termios.tcgetattr(slave)
    with tempfile.TemporaryDirectory(prefix='nexus-graphics-') as tmp:
        env = dict(os.environ, TERM='xterm-kitty', XDG_CONFIG_HOME=tmp, XDG_DATA_HOME=tmp)
        env.pop('TMUX', None)
        env.pop('STY', None)
        proc = subprocess.Popen([binary, '--fresh', '--chart-renderer', mode], stdin=slave, stdout=slave, stderr=slave, env=env)
        output = bytearray()
        def drain(seconds):
            end = time.monotonic() + seconds
            while time.monotonic() < end:
                if select.select([master], [], [], max(0, min(.05, end-time.monotonic())))[0]:
                    try:
                        output.extend(os.read(master, 65536))
                    except OSError:
                        break
        def send(keys, seconds=.3):
            os.write(master, keys)
            drain(seconds)
        try:
            drain(1.3)
            if mode == 'kitty':
                assert images(output), 'No complete chart PNG received'
                send(b' ')
                assert b'PAUSED' in output, 'Graph freeze not visible'
                output.clear()
                drain(2.4)
                assert not images(output), 'Frozen chart re-encoded while live collection continued'
                send(b'?')
                assert b'HELP' in output and b'a=d,d=I' in output, 'Modal did not remove chart overlay'
                output.clear()
                send(b'\x1b')
                assert images(output), 'Charts not restored after modal'
                output.clear()
                fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', 24, 80, 800, 552))
                drain(.5)
                resized = images(output)
                assert resized and all(int(m[b'c']) < 80 and int(m[b'r']) < 24 for m, _ in resized)
                output.clear()
                send(b']')
                assert images(output), 'Range change did not repaint frozen data'
            else:
                assert b'\x1b_G' not in output, 'Text mode emitted graphics escapes'
            send(b'q')
            proc.wait(timeout=3)
            after = termios.tcgetattr(slave)
            assert proc.returncode == 0
            assert after[3] & (termios.ICANON|termios.ECHO) == before[3] & (termios.ICANON|termios.ECHO)
            assert not os.listdir(tmp), '--fresh wrote settings or history'
            if mode == 'text':
                assert b'\x1b_G' not in output, 'Text exit emitted graphics escapes'
        finally:
            if proc.poll() is None:
                proc.kill()
                proc.wait()
    os.close(master)
    os.close(slave)
print('Graphics PTY passed: PNG transport, stable cache, graph freeze, modal cleanup, resize, range control, text fallback and terminal restoration')
