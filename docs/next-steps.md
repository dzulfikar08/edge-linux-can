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

## 3. Software quality in the image: done (2026-11-11), with gaps

Implemented: can-demo has a `-ptest` package (`ptest-cargo`) whose 13 Rust
tests pass on the device under `ptest-runner`; a test image
(`edge-image-test`) runs oe-core's `testimage` with the built-in ping, ssh,
date, systemd, ptest and parselogs cases plus our own `edge_can` case
(19 passed, 6 skipped, 0 failed); every image gets an SPDX 3.0 SBOM and a
CVE report from `sbom-cve-check` (the old `cve-check` class no longer
exists); can-demo is fetched from git at a pinned `SRCREV`; the DBC
describes the FD frames and the tests decode with it. How it works:
[learning-guide.md, part 3](learning-guide.md#part-3-tests-in-the-image-sbom-and-cves-pinned-sources-can-fd);
what happened: [build-journal.md, part 3](build-journal.md#part-3-tests-sbom-and-cves-can-fd).

Differences from the plan: testimage **complements** the scripts instead of
replacing them. It took over the single-boot checks (as `edge_can.py`); the
A/B test stays a script because it needs several reboots of one disk with
U-Boot's environment kept, and the smoke test stays because it checks the
product image (no ssh) over the serial console and can switch CAN
controllers. ptest runs in a separate test image, not in the product.

Still open:

1. **Triage the CVE report.** 735 unpatched entries, 594 of them kernel.
   Move the kas pins to the newest Wrynose point release (and 6.18 stable
   kernel) and rerun; then go through the user-space list (curl, openssl,
   glibc, expat, u-boot, busybox) and record decisions with `CVE_STATUS`
   in bbappends, so "unpatched" means "needs work" again.
2. **A distro without `x11`.** Poky's `x11` distro feature pulls libx11
   into this headless image through `dbus`. A small distro config of our own
   (`DISTRO = "edge"`, based on poky, without x11/wayland/opengl/vulkan)
   would remove it and shrink the image; it means rebuilding most packages.
3. **Fail the build on new critical CVEs**: a small check after
   `do_sbom_cve_check` that compares the report with an accepted list.
4. **Run the CVE scan on a schedule**, not only on builds: the databases
   change daily, a shipped release does not. Bumping the pinned database
   commits and rerunning only `do_sbom_cve_check` for a released image
   is enough.
5. **testimage for the A/B story**: a custom oeqa target that keeps the
   disk and U-Boot environment across reboots would let the A/B test use
   oeqa's reporting too. Not worth it while the script works.
6. **Licence compliance output**: `INHERIT += "archiver"` and the licence
   manifest, for the source-code obligations of GPL components.

## 4. CAN FD: done (2026-11-11), with gaps

QEMU's `ctucan_pci` replaced `kvaser_pci` in the smoke test and testimage;
`can-bitrate@can0` runs `/usr/libexec/can-up`, which sets
`bitrate 500000 dbitrate 2000000 fd on` and falls back to classic CAN on
controllers without FD; can-demo sends classic heartbeats plus 64-byte FD
telemetry (BRS) and receives both. Verified in both directions over host
`vcan0`, with the DBC, and the classic fallback with `kvaser_pci`. The
zephyr-bms-can project's test (36/36) and edge demo (13/13) still pass, also
with FD traffic on the shared bus.

Still open:

1. **Real FD hardware**: a controller and transceiver (e.g. an MCP2518FD
   on a Raspberry Pi, or i.MX 8M Plus FlexCAN FD), where the data-phase bit
   timing, sample points and transmitter delay compensation (`tdcv`,
   `tdco`) actually matter.
2. **zephyr-bms-can's edge demo** still boots its own older copy of the image
   (kvaser, classic). Refreshing that copy would show the new image against
   the BMS; the classic fallback it would need is tested here.
3. **QEMU's controller state**: the CTU CAN FD model never raises the
   fault-confinement interrupt, so Linux reports `STOPPED`. Could be fixed
   in QEMU (raise FCSI when the core is enabled); not done.
