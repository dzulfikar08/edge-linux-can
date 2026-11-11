# Next steps

Ordered roughly by value for an edge device. Nothing in this file is
implemented unless it says so.

## 1. A/B updates with RAUC: done (2026-11-11), with gaps

Implemented and tested in QEMU as planned here: meta-rauc (wrynose), U-Boot
with a boot-counting script, the wic layout (boot, rootfs A/B, data), the
read-only rootfs with `/etc` and `/var/lib` overlays on `/data`, a
development signing CA, verity bundles, and a health check that lets
`rauc-mark-good` mark a slot good or reboots. `scripts/qemu-ab.sh` passes
24/24 checks, including refusal of foreign-signed and wrong-compatible
bundles and a rollback from a broken release. How it works:
[learning-guide.md, part 2](learning-guide.md#part-2-read-only-root-and-ab-updates);
what happened: [build-journal.md, part 2](build-journal.md#part-2-read-only-root-and-ab-updates).

Differences from the original plan: the device tree is QEMU's own (passed
on by U-Boot), not one dumped into the rootfs; the boot script's partition
is found by U-Boot's standard boot instead of a patched `boot_targets`;
`/var/log` is a symlink to `/data/log`; a foreign-signed bundle is made
with openssl because `rauc resign` needs root in the build container.

Still open, roughly by value:

1. **Remote updates.** Today the test downloads a bundle with `wget` and
   runs `rauc install` on the console. Next: RAUC's HTTP streaming install
   (needs a server with Range requests, e.g. nginx), then
   `rauc-hawkbit-updater` (in meta-rauc) against a local hawkBit server in
   Docker, so the device polls for and reports updates.
2. **Provoke the other rollback paths.** A release that hangs (stops feeding
   the watchdog) and one whose kernel panics should also fall back after
   three attempts. Both mechanisms are configured (`RuntimeWatchdogSec`,
   `panic=10`) but untested.
3. **Faster rollback when the check is sure.** `rauc status mark-bad` in
   the health check for failures that cannot be transient, so the device
   falls back after one boot instead of three.
4. **Boot script in U-Boot's default environment** instead of a file on a
   FAT partition, and a decision on what happens when both slots fail (a
   small recovery/rescue image instead of retrying forever).
5. **`/data` integrity.** Run `e2fsck -p` on `/data` in the early init script
   before mounting it; decide what to do if it is corrupt (reformat and
   start with empty state?).
6. **`/etc` overlay migration.** Files changed on the device hide the
   versions a new release ships. Either keep configuration out of `/etc`
   (in `/data/config`), or have a post-install hook that resolves it.
7. **Keys like a product.** Signing in a separate step with the key on a
   token (PKCS#11); a production CA that development devices may also
   trust; a bundle that rotates the keyring.
8. **Bootloader updates** (U-Boot as its own slot class), only on hardware
   where that is possible safely.

## 2. Real hardware

Same layer on a board with a CAN controller: e.g. Raspberry Pi 4 with an
MCP2515 HAT (`meta-raspberrypi`, `dtoverlay=mcp2515-can0`), or an NXP i.MX 8M
board with FlexCAN (`meta-freescale`). Steps: add the BSP layer, set
`MACHINE`, add the CAN driver to the kernel fragment if the BSP kernel lacks
it. `can-bitrate@can0` and `can-demo@can0` work unchanged because they start
when `can0` appears.

## 3. Software quality in the image

- `ptest` for can-demo (`inherit ptest-cargo`) so `cargo test` runs on the
  target with `ptest-runner`.
- `oe-selftest`-style image tests: `IMAGE_CLASSES += "testimage"` and
  `bitbake -c testimage edge-image` instead of the custom smoke script, with
  a test case under `lib/oeqa/runtime/cases/` that checks `candump`.
- CVE scanning: `INHERIT += "cve-check"` (or the newer `sbom-cve-check`) and
  SPDX SBOM output, both built into oe-core.
- Pin `can-demo` by git `SRCREV` instead of `file://` so each image records
  the exact app commit.
- A small DBC file for the heartbeat (`0x100`/`0x101`: `seq` u32 LE,
  `magic` "HBT1") so CAN tools decode it by signal (done: `docs/can-demo.dbc`).

## 4. CAN FD

QEMU's `ctucan_pci` device is CAN FD capable and the driver is already in the
kernel fragment. Switch `kvaser_pci` for `ctucan_pci` in `qemu-smoke.py`,
set `ip link set can0 type can bitrate 500000 dbitrate 2000000 fd on`, and
extend can-demo to use `canfd_frame` (64-byte payload, `CAN_RAW_FD_FRAMES`
socket option). Host `vcan0` here has MTU 2060, so it already carries FD
frames.
