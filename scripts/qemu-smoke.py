#!/usr/bin/env python3
"""Boot edge-image in QEMU, log in on the serial console, run checks, power off.

Runs inside the kas container (`scripts/qemu-smoke.sh` starts it). The whole
serial console is written to --log. Exit status 0 means every check passed.

With --can-host IFACE the guest gets an emulated Kvaser PCI CAN controller
(guest can0) on a QEMU CAN bus that is bridged to the host SocketCAN interface
IFACE. That needs the container to share the host network namespace.
"""

import argparse
import os
import re
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from edgeqemu import Console, PROMPT  # noqa: E402


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--log", required=True)
    ap.add_argument("--image", default="edge-image")
    ap.add_argument("--can-host", help="host SocketCAN interface to bridge to guest can0")
    ap.add_argument("--host-id", default="321", help="CAN id (hex) the host sends, checked in the guest")
    ap.add_argument("--boot-timeout", type=int, default=900)
    ap.add_argument("--hold", type=int, default=0, help="keep the guest running N seconds before poweroff")
    args = ap.parse_args()

    # i6300esb: a PCI watchdog the guest kernel drives as /dev/watchdog0.
    qemuparams = ["-device", "i6300esb"]
    if args.can_host:
        qemuparams += ["-object", "can-bus,id=canbus0",
                      "-object", f"can-host-socketcan,id=canhost0,if={args.can_host},canbus=canbus0",
                      "-device", "kvaser_pci,canbus=canbus0"]
    # The image's qemuboot.conf makes runqemu boot u-boot.bin as firmware and
    # the wic disk as /dev/vda. snapshot: writes go to a temporary overlay, so
    # the deployed disk image stays as built (the A/B test, qemu-ab.py, uses
    # its own copy instead).
    argv = ["runqemu", "qemuarm64", args.image, "wic", "nographic", "slirp", "snapshot"]
    argv.append("qemuparams=" + " ".join(qemuparams))
    print("+", " ".join(argv), flush=True)

    con = Console(argv, args.log)
    results = []

    def check(name, ok, detail=""):
        results.append((name, ok))
        print(f"[{'PASS' if ok else 'FAIL'}] {name}" + (f"\n{detail.rstrip()}" if detail and not ok else ""), flush=True)

    t0 = time.monotonic()
    try:
        con.expect(rb"login: ", args.boot_timeout)
        check(f"boots to a login prompt ({time.monotonic() - t0:.0f}s)", True)
        con.send("root\n")
        con.expect(PROMPT, 60)
        con.run("dmesg -n 1")  # keep kernel messages off the console from here on

        rc, out = con.run("systemctl is-system-running --wait", 300)
        check("systemd reports 'running'", out.strip().endswith("running"), out + con.run("systemctl --failed --no-legend")[1])
        con.run("uname -a; head -n 3 /etc/os-release; cat /proc/cmdline")
        con.run("lsmod | grep -E '^(can|vcan|kvaser|sja1000)' || true")

        rc, out = con.run("awk '$2==\"/\"{o=$4} END{print o}' /proc/mounts; touch /usr/test-write")
        check("root filesystem is read-only", rc != 0 and out.splitlines()[0].startswith("ro,"), out)
        rc, out = con.run("awk '$2==\"/data\"||$2==\"/etc\"||$2==\"/var/lib\"' /proc/mounts; readlink /var/log")
        check("/data mounted, /etc is an overlay, /var/log -> /data/log",
              "/dev/vda4" in out and "overlay" in out and "/data/log" in out, out)
        rc, out = con.run("rauc status --detailed")
        check("rauc status: slot A booted, slots A and B known",
              rc == 0 and re.search(r"Booted from: rootfs\.0 \(A\)", out) is not None and "rootfs.1" in out, out)
        rc, out = con.run("systemctl is-active edge-health.service rauc-mark-good.service", 120)
        check("health check passed and slot marked good", rc == 0, out)

        rc, out = con.run("ip -details link show vcan0")
        check("vcan0 exists and is UP", rc == 0 and ",UP" in out, out)
        rc, out = con.run("systemctl is-active vcan@vcan0.service can-demo@vcan0.service")
        check("vcan@vcan0 and can-demo@vcan0 active", rc == 0, out)
        rc, out = con.run("candump -n 3 -T 5000 -td vcan0", 30)
        check("candump vcan0 sees can-demo heartbeats (id 100)", rc == 0 and len(re.findall(r"vcan0\s+100\s+\[8\]", out)) == 3, out)

        if args.can_host:
            rc, out = con.run("ip -details -statistics link show can0")
            check("can0 (kvaser_pci) exists, UP, ERROR-ACTIVE", rc == 0 and ",UP" in out and "ERROR-ACTIVE" in out, out)
            rc, out = con.run("systemctl is-active can-bitrate@can0.service can-demo@can0.service")
            check("can-bitrate@can0 and can-demo@can0 active", rc == 0, out)
            rc, out = con.run(f"candump -n 6 -T 5000 -td can0,{args.host_id}:7FF,101:7FF", 30)
            check(f"candump can0 sees own heartbeat (101) and host frames ({args.host_id})",
                  re.search(r"can0\s+101\s+\[8\]", out) is not None
                  and re.search(rf"can0\s+{args.host_id.upper()}\s+\[", out) is not None, out)

        rc, out = con.run("cat /sys/class/watchdog/watchdog0/identity /sys/class/watchdog/watchdog0/state "
                          "/sys/class/watchdog/watchdog0/timeout; systemctl show -p RuntimeWatchdogUSec")
        check("systemd drives the i6300esb hardware watchdog",
              "i6300ESB" in out and re.search(r"^active$", out, re.M) is not None and "RuntimeWatchdogUSec=30s" in out, out)

        con.run("sleep 11")  # let can-demo log one statistics line
        rc, out = con.run("journalctl -b -o short-monotonic --no-pager -u 'can-demo@*' | tail -n 12")
        check("can-demo logs frame statistics to the journal", "listener: frames total=" in out, out)
        con.run("systemctl show -p WatchdogUSec -p NRestarts -p MainPID can-demo@vcan0.service")
        con.run("systemctl list-units --no-pager 'can*' 'vcan*'")
        con.run("df -h / ; free -m")

        if args.hold:
            print(f"holding the guest for {args.hold}s", flush=True)
            con.run(f"sleep {args.hold}", args.hold + 60)
        con.send("poweroff\n")
        con.expect(rb"(reboot: Power down|Power down)", 180)
        check("clean poweroff", True)
    except (TimeoutError, EOFError) as e:
        check("console session", False, str(e))
    finally:
        con.close()

    failed = [n for n, ok in results if not ok]
    print(f"\n{len(results) - len(failed)}/{len(results)} checks passed", flush=True)
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
