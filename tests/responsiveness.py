"""Verify input, rendering and cancellation while a real HTTP request is blocked."""
import fcntl, os, pty, select, socket, struct, subprocess, sys, tempfile, termios, threading, time
server = socket.socket(); server.bind(('127.0.0.1', 0)); server.listen(1)
accepted = threading.Event(); stop = threading.Event()
def serve():
    conn, _ = server.accept()
    conn.recv(2048); accepted.set(); stop.wait(10); conn.close()
worker = threading.Thread(target=serve, daemon=True); worker.start()
master, slave = pty.openpty()
fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', 36, 130, 0, 0))
before = termios.tcgetattr(slave)
with tempfile.TemporaryDirectory(prefix='nexus-responsive-') as tmp:
    env=dict(os.environ, TERM='xterm-256color', XDG_CONFIG_HOME=tmp, XDG_DATA_HOME=tmp)
    proc=subprocess.Popen([os.path.abspath(sys.argv[1]),'--fresh'],stdin=slave,stdout=slave,stderr=slave,env=env)
    output=bytearray()
    def drain(seconds):
        end=time.monotonic()+seconds
        while time.monotonic()<end:
            if select.select([master],[],[],max(0,min(.05,end-time.monotonic())))[0]:
                try:output.extend(os.read(master,65536))
                except OSError:break
    def send(data,delay=.2):os.write(master,data);drain(delay)
    try:
        drain(1)
        send(b'\x0b');send(b'Inspect HTTP / HTTPS URL');send(b'\r')
        send(b'\x15'+f'http://127.0.0.1:{server.getsockname()[1]}/'.encode())
        send(b'\r');send(b'\r',.5) # explicit target consent
        assert accepted.wait(1),'HTTP job did not reach local server'
        start=time.monotonic(); output.clear(); send(b'?',.3)
        assert b'Keyboard shortcuts' in output and time.monotonic()-start<.6,'Help blocked behind network I/O'
        send(b'\x1b');output.clear();send(b'x',.4)
        assert b'cancelled' in output,'Request cancellation was not responsive'
        send(b'q');proc.wait(timeout=3)
        after=termios.tcgetattr(slave)
        assert proc.returncode==0
        assert after[3]&(termios.ICANON|termios.ECHO)==before[3]&(termios.ICANON|termios.ECHO)
        print('Responsiveness passed: help opens during stalled HTTP, cancellation works, terminal restored')
    finally:
        stop.set();server.close()
        if proc.poll() is None:proc.kill();proc.wait()
os.close(master);os.close(slave)
