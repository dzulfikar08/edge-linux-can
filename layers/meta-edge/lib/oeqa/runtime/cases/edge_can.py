#
# SPDX-License-Identifier: MIT
#
"""Runtime tests for the edge image: CAN, can-demo, read-only root, RAUC.

Run by `bitbake -c testimage edge-image-test` (TEST_SUITES contains
"edge_can"; see scripts/testimage.sh). Commands run on the guest over ssh.
Frames are decoded with the project's DBC (docs/can-demo.dbc) through
scripts/minidbc.py, both found via EDGE_REPO_DIR (layer.conf).

With EDGE_CAN_HOST set (kas/testimage-can.conf, host vcan0 up), the guest has
a CTU CAN FD controller bridged to that host interface; then this test also
plays the host side: it sends classic and FD frames on the host interface and
checks the guest receives them, and checks the guest's frames reach the host.
"""

import importlib.util
import json
import os
import re
import socket
import struct
import threading
import time

from oeqa.core.decorator.depends import OETestDepends
from oeqa.runtime.case import OERuntimeTestCase

HBT1 = int.from_bytes(b"HBT1", "little")
FDT1 = int.from_bytes(b"FDT1", "little")
# candump -L: "(1700000000.000000) vcan0 100#0011223344556677" (classic) or
# "(…) can0 201##1<hex>" (FD; the digit after ## is the flags nibble, 1 = BRS).
CANDUMP_L = re.compile(r"^\(\S+\)\s+(\w+)\s+([0-9A-F]+)(##?)([0-9A-F]?)([0-9A-F]*)\s*$", re.M)

CLASSIC = "=IB3x8s"   # struct can_frame
FD = "=IBB2x64s"      # struct canfd_frame
CANFD_BRS = 0x01


def load_dbc(repo):
    spec = importlib.util.spec_from_file_location("minidbc", os.path.join(repo, "scripts", "minidbc.py"))
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod.Dbc(os.path.join(repo, "docs", "can-demo.dbc"))


def parse_candump(out):
    """[(iface, id, is_fd, flags, bytes)] from candump -L output."""
    frames = []
    for iface, cid, sep, flags, data in CANDUMP_L.findall(out):
        fd = sep == "##"
        frames.append((iface, int(cid, 16), fd, int(flags or "0", 16), bytes.fromhex(data if fd else flags + data)))
    return frames


def telemetry_intact(data):
    return len(data) == 64 and data[4:8] == b"FDT1" and all(data[32 + i] == (data[0] + i) & 0xFF for i in range(32))


class EdgeCanTest(OERuntimeTestCase):

    @classmethod
    def setUpClass(cls):
        cls.dbc = load_dbc(cls.td["EDGE_REPO_DIR"])
        cls.can_host = (cls.td.get("EDGE_CAN_HOST") or "").strip()

    def run_ok(self, cmd, timeout=60):
        status, out = self.target.run(cmd, timeout)
        self.assertEqual(status, 0, msg=f"{cmd}\n{out}")
        return out

    def wait_active(self, units, seconds=120):
        deadline = time.monotonic() + seconds
        while True:
            status, out = self.target.run(f"systemctl is-active {units}")
            if status == 0 or time.monotonic() > deadline:
                return status, out
            time.sleep(2)

    # --- system: read-only root, data partition, RAUC -----------------------

    @OETestDepends(["ssh.SSHTest.test_ssh"])
    def test_readonly_root_and_data(self):
        out = self.run_ok("awk '$2==\"/\"{print $4}' /proc/mounts")
        self.assertTrue(out.strip().startswith("ro"), msg=f"root mount options: {out}")
        status, out = self.target.run("touch /usr/oeqa-write-test")
        self.assertNotEqual(status, 0, msg="writing to /usr succeeded on a read-only root")
        self.assertIn("Read-only file system", out)
        mounts = self.run_ok("cat /proc/mounts")
        self.assertRegex(mounts, r"(?m)^/dev/vda4 /data ext4 rw", msg=mounts)
        self.assertRegex(mounts, r"(?m)^\S+ /etc overlay rw", msg=mounts)
        self.assertRegex(mounts, r"(?m)^\S+ /var/lib overlay rw", msg=mounts)
        self.assertEqual(self.run_ok("readlink /var/log").strip(), "/data/log")

    @OETestDepends(["ssh.SSHTest.test_ssh"])
    def test_rauc_status(self):
        status, out = self.wait_active("edge-health.service rauc-mark-good.service")
        self.assertEqual(status, 0, msg=f"health check / mark-good not active: {out}")
        # meta-rauc installs the configuration in /usr/lib/rauc (read-only root).
        compatible = re.search(r"^compatible=(.+)$", self.run_ok("cat /usr/lib/rauc/system.conf"), re.M).group(1).strip()
        st = json.loads(self.run_ok("rauc status --detailed --output-format=json"))
        self.assertEqual(st["compatible"], compatible)
        self.assertTrue(compatible.endswith("edge-qemuarm64"), msg=compatible)
        self.assertEqual(st["booted"], "A")
        slots = {name: info for entry in st["slots"] for name, info in entry.items()}
        self.assertEqual(sorted(slots), ["rootfs.0", "rootfs.1"])
        self.assertEqual(slots["rootfs.0"]["state"], "booted")
        self.assertEqual(slots["rootfs.0"]["boot_status"], "good")
        self.assertEqual(slots["rootfs.0"]["device"], "/dev/vda2")
        self.assertEqual(slots["rootfs.1"]["device"], "/dev/vda3")

    # --- CAN inside the guest (vcan0) ----------------------------------------

    @OETestDepends(["ssh.SSHTest.test_ssh"])
    def test_vcan0_up_and_services(self):
        out = self.run_ok("ip -details link show vcan0")
        self.assertIn(",UP", out)
        # 72 = CAN FD; Linux 6.18 creates a vcan with 2060 (CAN XL), which carries FD too.
        mtu = int(re.search(r"mtu (\d+) ", out).group(1))
        self.assertGreaterEqual(mtu, 72, msg="guest vcan0 should carry CAN FD frames")
        status, out = self.wait_active("vcan@vcan0.service can-demo@vcan0.service")
        self.assertEqual(status, 0, msg=out)

    @OETestDepends(["edge_can.EdgeCanTest.test_vcan0_up_and_services"])
    def test_vcan0_heartbeat_payload(self):
        out = self.run_ok("candump -L -n 3 -T 5000 vcan0,100:7FF", 30)
        frames = [f for f in parse_candump(out) if f[1] == 0x100]
        self.assertEqual(len(frames), 3, msg=out)
        seqs = []
        for _, cid, fd, _, data in frames:
            self.assertFalse(fd, msg="the heartbeat is a classic CAN frame")
            name, sig = self.dbc.decode(cid, data)
            self.assertEqual(name, "EdgeHeartbeatVcan")
            self.assertEqual(len(data), 8)
            self.assertEqual(sig["Magic"], HBT1, msg=f"payload {data.hex()}")
            seqs.append(sig["Seq"])
        self.assertEqual(seqs, [seqs[0], seqs[0] + 1, seqs[0] + 2], msg=f"sequence numbers {seqs}")

    @OETestDepends(["edge_can.EdgeCanTest.test_vcan0_up_and_services"])
    def test_vcan0_fd_telemetry(self):
        out = self.run_ok("candump -L -n 2 -T 5000 vcan0,200:7FF", 30)
        frames = [f for f in parse_candump(out) if f[1] == 0x200]
        self.assertEqual(len(frames), 2, msg=out)
        for _, cid, fd, flags, data in frames:
            self.assertTrue(fd, msg="telemetry must be a CAN FD frame")
            self.assertTrue(flags & CANFD_BRS, msg="telemetry is sent with bit rate switch")
            self.assertEqual(len(data), 64)
            self.assertTrue(telemetry_intact(data), msg=data.hex())
            name, sig = self.dbc.decode(cid, data)
            self.assertEqual(name, "EdgeTelemetryFdVcan")
            self.assertEqual(sig["Magic"], FDT1)
            self.assertGreater(sig["UptimeMs"], 0)
            self.assertGreaterEqual(sig["StartCount"], 1)

    @OETestDepends(["edge_can.EdgeCanTest.test_vcan0_up_and_services"])
    def test_journal_counters(self):
        # can-demo logs a statistics line every 10 s.
        cmd = "journalctl -b -u can-demo@vcan0 -o cat --no-pager | grep 'listener: frames total=' | tail -n 1"
        deadline = time.monotonic() + 30
        line = ""
        while not line and time.monotonic() < deadline:
            line = self.run_ok(cmd).strip()
            if not line:
                time.sleep(3)
        m = re.search(r"frames total=(\d+) errors=(\d+) fd=(\d+) fd_bad=(\d+) heartbeat_last_seq=(\d+) heartbeat_missed=(\d+)", line)
        self.assertIsNotNone(m, msg=f"no statistics line: {line!r}")
        total, errors, fd, fd_bad, _, missed = map(int, m.groups())
        self.assertGreater(total, 0)
        self.assertGreater(fd, 0, msg=line)
        self.assertEqual((errors, fd_bad, missed), (0, 0, 0), msg=line)

    # --- CAN FD controller bridged to the host --------------------------------

    def need_bridge(self):
        if not self.can_host:
            self.skipTest("EDGE_CAN_HOST not set (no CAN bridge to the host)")

    @OETestDepends(["ssh.SSHTest.test_ssh"])
    def test_can0_fd_mode(self):
        self.need_bridge()
        status, out = self.wait_active("can-bitrate@can0.service can-demo@can0.service")
        self.assertEqual(status, 0, msg=out)
        out = self.run_ok("ip -details link show can0")
        self.assertIn(",UP", out)
        self.assertRegex(out, r"mtu 72 ", msg=out)
        # ERROR-ACTIVE on hardware. QEMU's CTU CAN FD model never raises the
        # fault-confinement interrupt that moves the driver out of STOPPED;
        # test_bridge_both_directions shows the controller works anyway.
        self.assertRegex(out, r"can <FD> state (ERROR-ACTIVE|STOPPED)", msg=out)
        self.assertRegex(out, r"bitrate 500000 ", msg=out)
        self.assertRegex(out, r"dbitrate 2000000 ", msg=out)
        self.assertIn("ctucanfd", self.run_ok("basename $(readlink /sys/class/net/can0/device/driver)"))

    @OETestDepends(["edge_can.EdgeCanTest.test_can0_fd_mode"])
    def test_bridge_both_directions(self):
        """Host vcan0 <-> guest can0, classic and FD frames in both directions."""
        sock = socket.socket(socket.AF_CAN, socket.SOCK_RAW, socket.CAN_RAW)
        sock.setsockopt(socket.SOL_CAN_RAW, socket.CAN_RAW_FD_FRAMES, 1)
        sock.bind((self.can_host,))
        sock.settimeout(0.1)
        received = {}   # id -> list of (is_fd, flags, data)
        stop = threading.Event()

        def host_side():
            n = 0
            next_tx = 0.0
            while not stop.is_set():
                if time.monotonic() >= next_tx:
                    sock.send(struct.pack(CLASSIC, 0x321, 8, b"HOST" + struct.pack("<I", n)))
                    head = b"HOSTFD01" + struct.pack("<I", n)
                    sock.send(struct.pack(FD, 0x322, 64, CANFD_BRS, head + bytes((n + i) & 0xFF for i in range(52))))
                    n += 1
                    next_tx = time.monotonic() + 0.25
                try:
                    raw = sock.recv(72)
                except socket.timeout:
                    continue
                if len(raw) == 72:
                    cid, ln, flags, data = struct.unpack(FD, raw)
                    received.setdefault(cid & 0x7FF, []).append((True, flags, data[:ln]))
                elif len(raw) == 16:
                    cid, ln, data = struct.unpack(CLASSIC, raw)
                    received.setdefault(cid & 0x7FF, []).append((False, 0, data[:ln]))

        t = threading.Thread(target=host_side, daemon=True)
        t.start()
        try:
            out = self.run_ok("candump -L -n 6 -T 10000 can0,321:7FF,322:7FF", 40)
            time.sleep(2)  # a couple more guest frames on the host side
        finally:
            stop.set()
            t.join(5)
            sock.close()

        # Guest received the host's frames.
        guest = parse_candump(out)
        classic = [f for f in guest if f[1] == 0x321 and not f[2]]
        fd = [f for f in guest if f[1] == 0x322 and f[2]]
        self.assertTrue(classic and fd, msg=f"guest can0 saw:\n{out}")
        for _, cid, _, flags, data in fd:
            self.assertEqual(len(data), 64)
            self.assertTrue(flags & CANFD_BRS)
            name, sig = self.dbc.decode(cid, data)
            self.assertEqual(name, "HostFdPing")
            self.assertEqual(data[:8], b"HOSTFD01")
            self.assertEqual(sig["PatternLast"], (sig["Count"] + 51) & 0xFF)

        # Host received the guest's classic heartbeat and FD telemetry.
        hb = received.get(0x101, [])
        tlm = received.get(0x201, [])
        self.assertTrue(hb, msg=f"no heartbeat 0x101 on host {self.can_host}; got ids {sorted(map(hex, received))}")
        self.assertTrue(tlm, msg=f"no FD telemetry 0x201 on host {self.can_host}; got ids {sorted(map(hex, received))}")
        for is_fd, _, data in hb:
            self.assertFalse(is_fd)
            self.assertEqual(self.dbc.decode(0x101, data)[1]["Magic"], HBT1)
        for is_fd, flags, data in tlm:
            self.assertTrue(is_fd and flags & CANFD_BRS)
            self.assertTrue(telemetry_intact(data), msg=data.hex())
            name, sig = self.dbc.decode(0x201, data)
            self.assertEqual((name, sig["Magic"]), ("EdgeTelemetryFd", FDT1))
