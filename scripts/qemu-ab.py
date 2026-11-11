#!/usr/bin/env python3
"""A/B update test: install, reboot into the new slot, roll back from a bad one.

Runs inside the kas container (`scripts/qemu-ab.sh` starts it). Boots a fresh
copy of the edge-image disk under U-Boot, with U-Boot's environment in a file
that persists across the guest's reboots (QEMU flash bank 1), and goes through:

  1. Boot slot A (release 1.0.0). Check: read-only root, /data, RAUC status,
     health check, slot marked good. Leave a marker file on /data.
     Bundles signed by an untrusted CA or made for another product
     (wrong "compatible") must be refused.
  2. Download edge-bundle (1.1.0) over HTTP, `rauc install`, reboot.
  3. Slot B runs 1.1.0. The marker, the machine id, the journal of the
     earlier boot and can-demo's start counter survived. Slot B marked good.
  4. Install edge-bundle-broken (1.2.0-broken, can-demo cannot start) into
     slot A, reboot.
  5. Slot A fails its health check on every boot; U-Boot's counter runs out
     after 3 attempts and it boots slot B again: release 1.1.0, healthy,
     slot A shown as bad.

The serial console of every boot is written to --log. Exit status 0 means
every check passed.
"""

import argparse
import functools
import http.server
import os
import re
import shutil
import subprocess
import sys
import threading
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from edgeqemu import Console  # noqa: E402

DEPLOY = "/build/tmp/deploy/images/qemuarm64"
WORK = "/build/tmp/ab-test"
GOOD = "edge-bundle-qemuarm64.raucb"
BROKEN = "edge-bundle-broken-qemuarm64.raucb"
# Made from GOOD by scripts/forge-bundles.py (qemu-ab.sh runs it on the host):
FORGED = "/build/tmp/ab-forged"
FOREIGN = "foreign-signed.raucb"      # signed by keys/dev/other-ca
WRONG = "wrong-compatible.raucb"      # valid signature, compatible=some-other-product


def qemu_binary():
    """The qemu-system-aarch64 that Yocto built (same one runqemu uses)."""
    conf = open(f"{DEPLOY}/edge-image-qemuarm64.rootfs.qemuboot.conf").read()
    rel = re.search(r"^staging_bindir_native = (.+)$", conf, re.M).group(1).strip()
    return os.path.normpath(os.path.join(DEPLOY, rel, "qemu-system-aarch64"))


def serve(directory):
    """HTTP server for the bundles. QEMU's user-mode network (slirp) maps
    10.0.2.2 in the guest to 127.0.0.1 here."""
    handler = functools.partial(http.server.SimpleHTTPRequestHandler, directory=directory)
    handler.log_message = lambda *a: None
    srv = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
    threading.Thread(target=srv.serve_forever, daemon=True).start()
    return srv.server_address[1]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--log", required=True)
    ap.add_argument("--boot-timeout", type=int, default=900)
    args = ap.parse_args()

    # A fresh disk and a blank U-Boot environment (64 MiB = one flash bank).
    shutil.rmtree(WORK, ignore_errors=True)
    os.makedirs(WORK)
    disk = f"{WORK}/disk.wic"
    env = f"{WORK}/u-boot-env.img"
    shutil.copyfile(f"{DEPLOY}/edge-image-qemuarm64.rootfs.wic", disk)
    with open(env, "wb") as f:
        f.truncate(64 * 1024 * 1024)
    for b in (GOOD, BROKEN):
        os.link(os.path.realpath(f"{DEPLOY}/{b}"), f"{WORK}/{b}")
    for b in (FOREIGN, WRONG):
        shutil.copyfile(f"{FORGED}/{b}", f"{WORK}/{b}")
    port = serve(WORK)
    url = f"http://10.0.2.2:{port}"

    argv = [qemu_binary(),
            "-machine", "virt", "-cpu", "cortex-a57", "-smp", "4", "-m", "512",
            "-nographic",
            # Firmware: U-Boot. Its environment lives in flash bank 1, backed by a file.
            "-bios", f"{DEPLOY}/u-boot.bin",
            "-drive", f"if=pflash,format=raw,index=1,file={env}",
            # The disk: boot, rootfs A, rootfs B, data  ->  /dev/vda1..4
            "-drive", f"if=none,id=disk0,format=raw,file={disk}",
            "-device", "virtio-blk-pci,drive=disk0",
            "-netdev", "user,id=net0", "-device", "virtio-net-pci,netdev=net0",
            "-object", "rng-random,filename=/dev/urandom,id=rng0", "-device", "virtio-rng-pci,rng=rng0",
            "-device", "i6300esb"]
    print("+", " ".join(argv), flush=True)
    con = Console(argv, args.log)
    results = []

    def check(name, ok, detail=""):
        results.append((name, ok))
        print(f"[{'PASS' if ok else 'FAIL'}] {name}" + (f"\n{detail.rstrip()}" if detail and not ok else ""), flush=True)
        return ok

    def show(cmd, timeout=60):
        """Run a command, print it and its output (for the docs), return it."""
        rc, out = con.run(cmd, timeout)
        print(f"  $ {cmd}\n" + "".join(f"    {line}\n" for line in out.strip().splitlines()), end="", flush=True)
        return rc, out

    def boot(label):
        t0 = time.monotonic()
        con.note(label)
        out = con.expect(rb"edge-boot: slot ([AB])[^\n]*", args.boot_timeout)
        line = out.decode(errors="replace").splitlines()[-1].strip()
        con.login(args.boot_timeout)
        print(f"--- {label}: {line} (login after {time.monotonic() - t0:.0f}s)", flush=True)
        con.run("systemctl is-system-running --wait", 300)
        return line

    def state():
        rc, out = show("sed -n 's/^VERSION=//p' /usr/lib/edge-release; sed 's/.*rauc.slot=\\([AB]\\).*/\\1/' /proc/cmdline")
        version, slot = (out.strip().splitlines() + ["", ""])[:2]
        return version.strip(), slot.strip()

    def uboot_env():
        return show("fw_printenv BOOT_ORDER BOOT_A_LEFT BOOT_B_LEFT")[1]

    def install(bundle):
        """Download a bundle to /data and install it; returns RAUC's output."""
        con.run(f"wget -q -O /data/update.raucb {url}/{bundle}", 300)
        show("rauc info /data/update.raucb 2>&1 | head -n 16", 300)
        rc, out = show("rauc install /data/update.raucb > /tmp/install.log 2>&1; echo \"exit=$?\"; tail -n 4 /tmp/install.log", 900)
        con.run("rm -f /data/update.raucb")
        return out

    t_start = time.monotonic()
    try:
        # ---- 1. first boot: slot A, release 1.0.0 -------------------------
        line = boot("boot 1: fresh disk")
        check("U-Boot picks slot A on a fresh disk", "slot A" in line, line)
        version, slot = state()
        check("running slot A, version 1.0.0", (slot, version) == ("A", "1.0.0"), f"{slot} {version}")
        rc, out = show("touch /usr/x 2>&1; touch /etc/x-test && rm /etc/x-test && echo etc-writable; "
                       "awk '$2==\"/\"' /proc/mounts")
        check("rootfs read-only, /etc writable (overlay)",
              "Read-only file system" in out and "etc-writable" in out and " ro," in out, out)
        con.run("systemctl is-active edge-health.service rauc-mark-good.service", 120)
        rc, out = show("rauc status --detailed 2>&1 | sed -n '1,30p'")
        check("rauc: booted from A, A marked good", "Booted from: rootfs.0 (A)" in out
              and re.search(r"rootfs\.0.*?boot status: good", out, re.S) is not None, out)
        env1 = uboot_env()
        check("U-Boot env readable from Linux, A has 3 attempts again", "BOOT_A_LEFT=3" in env1, env1)
        _, marker = show("cat /proc/sys/kernel/random/uuid | tee /data/ab-test-marker")
        marker = marker.strip()
        _, mid = show("cat /etc/machine-id")
        mid = mid.strip()
        _, starts1 = show("cat /var/lib/can-demo/vcan0.starts")
        show("journalctl -b -u edge-health --no-pager -o cat | tail -n 2")

        out = install(FOREIGN)
        check("bundle signed by an untrusted CA is refused",
              "exit=0" not in out and re.search(r"signature|verif|certificate", out, re.I) is not None, out)
        out = install(WRONG)
        check("bundle for another product (wrong compatible) is refused",
              "exit=0" not in out and "compatible" in out.lower(), out)

        # ---- 2. install the good update -----------------------------------
        out = install(GOOD)
        check("rauc install edge-bundle (1.1.0) succeeded", "Installing `/data/update.raucb` succeeded" in out, out)
        env2 = uboot_env()
        check("RAUC put slot B first in BOOT_ORDER", "BOOT_ORDER=B A" in env2, env2)
        con.send("reboot\n")

        # ---- 3. slot B, release 1.1.0 --------------------------------------
        line = boot("boot 2: after installing 1.1.0")
        version, slot = state()
        check("after reboot: slot B, version 1.1.0", (slot, version) == ("B", "1.1.0"), f"{slot} {version}")
        _, m = show("cat /data/ab-test-marker")
        check("data partition survived the update (marker file)", m.strip() == marker, m)
        _, m = show("cat /etc/machine-id")
        check("machine-id survived (kept in the /etc overlay on /data)", m.strip() == mid, m)
        _, starts2 = show("cat /var/lib/can-demo/vcan0.starts")
        check("can-demo start counter kept counting (/var/lib on /data)",
              starts2.strip().isdigit() and starts1.strip().isdigit() and int(starts2) > int(starts1),
              f"{starts1} -> {starts2}")
        _, out = show("journalctl --list-boots --no-pager | tail -n 3; journalctl -b -1 --no-pager -o cat -u edge-health | grep '^edge-health:'")
        check("journal of the previous boot (slot A) is readable", "slot A, version 1.0.0" in out, out)
        con.run("systemctl is-active edge-health.service rauc-mark-good.service", 120)
        rc, out = show("rauc status 2>&1 | sed -n '1,25p'")
        check("rauc: booted from B, B marked good", "Booted from: rootfs.1 (B)" in out, out)
        uboot_env()

        # ---- 4. install the broken release into slot A ---------------------
        out = install(BROKEN)
        check("rauc install edge-bundle-broken (1.2.0-broken) succeeded",
              "Installing `/data/update.raucb` succeeded" in out, out)
        env3 = uboot_env()
        check("RAUC put slot A first in BOOT_ORDER", "BOOT_ORDER=A B" in env3, env3)
        con.send("reboot\n")

        # ---- 5. slot A fails 3 times, U-Boot falls back to B ---------------
        con.note("boots 3-5: broken release in slot A")
        t0 = time.monotonic()
        out = con.expect(rb"edge-boot: slot B[^\n]*", 3 * args.boot_timeout).decode(errors="replace")
        tries = re.findall(r"edge-boot: slot A[^\n]*", out)
        unhealthy = re.findall(r"edge-health: UNHEALTHY[^\n]*", out)
        print(f"--- broken slot A booted {len(tries)} times before U-Boot chose B "
              f"({time.monotonic() - t0:.0f}s):", flush=True)
        for t in tries:
            print("    " + t.strip(), flush=True)
        check("U-Boot tried the broken slot A 3 times, then fell back to B", len(tries) == 3, out[-3000:])
        for u in unhealthy:
            print("    " + u.strip(), flush=True)
        check("each of those boots printed the failed health check on the console", len(unhealthy) == 3, out[-3000:])
        con.login(args.boot_timeout)
        con.run("systemctl is-system-running --wait", 300)
        print("--- boot 6: rollback", flush=True)
        version, slot = state()
        check("after rollback: slot B, version 1.1.0", (slot, version) == ("B", "1.1.0"), f"{slot} {version}")
        _, out = show("journalctl --list-boots --no-pager | tail -n 5; "
                      "journalctl --no-pager -o cat -u edge-health | grep -E 'UNHEALTHY|healthy' | tail -n 5")
        check("journal shows the unhealthy boots of 1.2.0-broken",
              len(re.findall(r"UNHEALTHY \(slot A, version 1\.2\.0-broken\)", out)) >= 3, out)
        con.run("systemctl is-active edge-health.service rauc-mark-good.service", 120)
        rc, out = show("rauc status --detailed 2>&1 | sed -n '1,40p'")
        check("rauc: booted from B, slot A bad", "Booted from: rootfs.1 (B)" in out
              and re.search(r"rootfs\.0.*?boot status: bad", out, re.S) is not None, out)
        uboot_env()
        _, m = show("cat /data/ab-test-marker")
        check("data partition still intact after the rollback", m.strip() == marker, m)
        show("df -h / /data | sed 's/  */ /g'")

        con.send("poweroff\n")
        con.expect(rb"(reboot: Power down|Power down)", 180)
        check("clean poweroff", True)
    except (TimeoutError, EOFError) as e:
        check("console session", False, str(e))
    finally:
        con.close()

    failed = [n for n, ok in results if not ok]
    print(f"\n{len(results) - len(failed)}/{len(results)} checks passed in {time.monotonic() - t_start:.0f}s", flush=True)
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
