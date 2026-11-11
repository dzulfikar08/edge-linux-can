#!/usr/bin/env python3
"""Boot edge-image with a given qemu-system-aarch64 and check the CTU CAN FD
controller: the test that was run on QEMU master with the patch before
preparing the submission (docs/upstream/SEND.md).

  python3 docs/upstream/boot-test.py --qemu /path/to/build/qemu-system-aarch64 \
      --deploy /mnt/yocto/build/tmp/deploy/images/qemuarm64 --log /tmp/master-boot.log

Runs on the host (not in the kas container). Needs host vcan0 up (MTU >= 72).
Uses scripts/edgeqemu.py for the serial console and scripts/host-can.py as
the host side of the bus. Exit status 0 = all checks passed.
"""
import argparse
import json
import os
import re
import subprocess
import sys
import tempfile
import time

REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
sys.path.insert(0, os.path.join(REPO, "scripts"))
from edgeqemu import Console  # noqa: E402


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--qemu", required=True)
    ap.add_argument("--deploy", required=True, help="deploy/images/qemuarm64 of the Yocto build")
    ap.add_argument("--log", required=True)
    ap.add_argument("--can-host", default="vcan0")
    args = ap.parse_args()

    wic = os.path.join(args.deploy, "edge-image-qemuarm64.rootfs.wic")
    uboot = os.path.join(args.deploy, "u-boot.bin")
    argv = [args.qemu, "-machine", "virt", "-cpu", "cortex-a57", "-smp", "4", "-m", "512",
            "-bios", uboot, "-drive", f"if=virtio,file={wic},format=raw", "-snapshot",
            "-nic", "none",  # not needed for these checks (and builds without libslirp have no "user" netdev)
            "-device", "i6300esb",
            "-object", "can-bus,id=canbus0",
            "-object", f"can-host-socketcan,id=canhost0,if={args.can_host},canbus=canbus0",
            "-object", "can-bus,id=canbus1",
            "-device", "ctucan_pci,canbus0=canbus0,canbus1=canbus1",
            "-nographic", "-serial", "mon:stdio"]

    host_out = tempfile.mktemp(suffix=".json")
    host = subprocess.Popen([sys.executable, "-I", os.path.join(REPO, "scripts", "host-can.py"),
                             "--iface", args.can_host, "--send-id", "321", "--send-fd-id", "322",
                             "--out", host_out])
    results = []

    def check(name, ok, detail=""):
        results.append((name, ok))
        print(f"[{'PASS' if ok else 'FAIL'}] {name}" + (f"\n{detail.rstrip()}" if detail and not ok else ""), flush=True)

    con = Console(argv, args.log)
    con.note("argv: " + " ".join(argv))
    try:
        rc, out = 0, ""
        t0 = time.monotonic()
        con.login(900)
        check(f"boots to a login prompt ({time.monotonic() - t0:.0f}s)", True)
        rc, out = con.run("systemctl is-system-running --wait", 300)
        check("systemd reports 'running'", out.strip().endswith("running"), out)
        rc, out = con.run("systemctl is-active can-bitrate@can0.service can-demo@can0.service", 120)
        check("can-bitrate@can0 and can-demo@can0 active", rc == 0, out)
        rc, out = con.run("ip -details link show can0")
        check("can0 UP, CAN FD, state ERROR-ACTIVE", ",UP" in out and "can <FD> state ERROR-ACTIVE " in out, out)
        rc, out = con.run("dmesg | grep -E 'can0: (ctucan_err_interrupt|state changes|.*Invalid error state)'")
        check("driver saw FCSI (ISR 0x10) and STOPPED -> ERROR_ACTIVE, no invalid state",
              "ISR = 0x00000010" in out and re.search(r"STOPPED to (CAN_STATE_)?ERROR_ACTIVE", out) is not None
              and "Invalid" not in out, out)
        rc, out = con.run("systemctl stop can-demo@can0.service can-bitrate@can0.service; "
                          "ip -details link show can0 | grep -oE 'can (<[A-Z,_-]*> )?state [A-Z-]+'; "
                          "systemctl start can-bitrate@can0.service can-demo@can0.service; "
                          "ip -details link show can0 | grep -oE 'can (<[A-Z,_-]*> )?state [A-Z-]+'; "
                          "dmesg | grep -c 'can0: state changes from'", 60)
        states = re.findall(r"^can (?:<[A-Z,_-]*> )?state ([A-Z-]+)$", out, re.M)
        check("STOPPED when down, ERROR-ACTIVE after up, one transition per start",
              states == ["STOPPED", "ERROR-ACTIVE"] and out.strip().endswith("2"), out)
        rc, out = con.run("candump -n 8 -T 10000 can0,101:7FF,201:7FF,321:7FF,322:7FF", 30)
        check("candump can0: own 101/201 and host 321 (classic) / 322 (FD, 64 bytes)",
              re.search(r"can0\s+101\s+\[8\]", out) is not None and re.search(r"can0\s+201\s+\[64\]", out) is not None
              and re.search(r"can0\s+321\s+\[8\]", out) is not None and re.search(r"can0\s+322\s+\[64\]", out) is not None, out)
        con.send("poweroff\n")
        con.expect(rb"(reboot: Power down|Power down)", 180)
        check("clean poweroff", True)
    except (TimeoutError, EOFError) as e:
        check("console session", False, str(e))
    finally:
        con.close()
        host.terminate()
        host.wait(10)

    try:
        d = json.load(open(host_out))
        check("host received guest heartbeat 0x101 and intact FD telemetry 0x201",
              d["received"].get("101", 0) > 0 and d["received_fd"].get("201", 0) > 0
              and d["telemetry"]["intact"] > 0 and d["telemetry"]["damaged"] == 0, json.dumps(d)[:400])
    except (OSError, ValueError, KeyError) as e:
        check("host side results", False, str(e))

    failed = [n for n, ok in results if not ok]
    print(f"\n{len(results) - len(failed)}/{len(results)} checks passed", flush=True)
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
