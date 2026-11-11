"""Serial console driver for the QEMU tests (qemu-smoke.py, qemu-ab.py).

Starts a command (runqemu or qemu-system-aarch64) on a pseudo-terminal,
answers the guest's terminal-size queries, waits for patterns, logs in and
runs shell commands with their exit status. Everything the guest prints is
kept and written, cleaned of escape sequences, to the log file on close().
"""

import os
import pty
import re
import select
import signal
import subprocess
import time

# Escape sequences: OSC/DCS (systemd 259 emits OSC 3008 context markers and a
# DCS terminal-name query) and CSI (colours, cursor moves).
_ESC = rb"(?:\x1b[\]P][^\x07\x1b]*(?:\x07|\x1b\\)|\x1b\[[0-9;:?!]*[A-Za-z]|\x1b[78])"
PROMPT = re.compile(rb"root@[\w-]+:[^\n#]*# " + _ESC + rb"*$")
# Cursor position query. The guest's getty and Poky's /etc/profile `resize`
# function send it to learn the terminal size and wait (with echo off) for the
# answer, so the console must reply like a real terminal would.
DSR = b"\x1b[6n"
ANY_ESC = re.compile(_ESC)


class Console:
    def __init__(self, argv, log_path):
        self.master, slave = pty.openpty()
        self.proc = subprocess.Popen(argv, stdin=slave, stdout=slave, stderr=slave,
                                     start_new_session=True, close_fds=True)
        os.close(slave)
        self.log_path = log_path
        self.raw = bytearray()  # whole console output, cleaned and saved on close()
        self.buf = b""
        self.tail = b""

    def _pump(self, timeout):
        r, _, _ = select.select([self.master], [], [], timeout)
        if not r:
            return False
        try:
            data = os.read(self.master, 65536)
        except OSError:
            data = b""
        if not data:
            raise EOFError("QEMU exited")
        scan = self.tail + data
        for _ in range(scan.count(DSR)):
            os.write(self.master, b"\x1b[50;200R")  # "cursor at row 50, column 200"
        self.tail = scan[-(len(DSR) - 1):]
        self.raw += data
        self.buf += data
        return True

    def note(self, text):
        """Add a line to the saved log (not sent to the guest)."""
        self.raw += f"\n##### {text}\n".encode()

    def login(self, timeout=900):
        """Wait for the login prompt, log in as root, quiet the kernel."""
        self.expect(rb"login: ", timeout)
        self.send("root\n")
        self.expect(PROMPT, 60)
        self.run("dmesg -n 1")

    def expect(self, pattern, timeout):
        rx = re.compile(pattern) if isinstance(pattern, bytes) else pattern
        deadline = time.monotonic() + timeout
        while True:
            m = rx.search(self.buf)
            if m:
                out = self.buf[:m.end()]
                self.buf = self.buf[m.end():]
                return out
            left = deadline - time.monotonic()
            if left <= 0:
                raise TimeoutError(f"timeout waiting for {rx.pattern!r}")
            self._pump(min(left, 1.0))

    def send(self, text):
        os.write(self.master, text.encode())

    def run(self, cmd, timeout=60):
        """Run a shell command in the guest and return (rc, output)."""
        # The quotes split the markers so the echoed command line never matches.
        self.send(f'echo "@@B""EGIN"; {cmd}; echo "@@E""ND rc=$?"\n')
        self.expect(rb"\n@@BEGIN\r*\n", timeout)  # "\r\r\n" once the profile has run
        out = self.expect(rb"@@END rc=(\d+)", timeout)
        rc = int(re.search(rb"@@END rc=(\d+)", out).group(1))
        self.expect(PROMPT, 10)
        # Colours and other escape sequences (rauc status uses them) are dropped.
        text = ANY_ESC.sub(b"", out[: out.rfind(b"@@END")]).decode(errors="replace").replace("\r", "")
        return rc, text

    def save(self):
        # The saved log keeps text only: escape sequences (colours, terminal
        # queries, systemd's OSC 3008 markers) and carriage returns are dropped.
        with open(self.log_path, "wb") as f:
            f.write(ANY_ESC.sub(b"", bytes(self.raw)).replace(b"\r", b""))

    def close(self, timeout=60):
        try:
            self.proc.wait(timeout)
        except subprocess.TimeoutExpired:
            os.killpg(self.proc.pid, signal.SIGTERM)
            self.proc.wait(10)
        try:
            while self._pump(0.2):
                pass
        except (EOFError, OSError):
            pass
        self.save()
