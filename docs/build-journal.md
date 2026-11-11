# Build journal

What was actually done, in order, between 2026-11-10 22:30 and 2026-11-11
08:35 (WIB), with the commands, the problems hit and how each was fixed. The
concepts behind each step are explained in [learning-guide.md](learning-guide.md).

Host: Arch Linux, 12 cores, 30 GB RAM, Docker 29.9. Build storage: an 84 GB
ext4 image on a USB SSD mounted at `/mnt/yocto`.

## 1. Choosing the release

```sh
git ls-remote --heads https://git.openembedded.org/openembedded-core   # scarthgap … whinlatter, wrynose
git ls-remote --tags  https://git.openembedded.org/openembedded-core 'yocto-6.0*'   # … yocto-6.0.4
git ls-remote --tags  https://git.openembedded.org/bitbake 'yocto-6.0.4*'
git ls-remote --tags  https://git.yoctoproject.org/meta-yocto 'yocto-6.0.4*'
git ls-remote https://git.openembedded.org/meta-openembedded refs/heads/wrynose
git ls-remote --tags https://github.com/siemens/kas            # newest: 5.5
```

Result: **Yocto 6.0.4 "Wrynose"** (LTS). The kas file pins bitbake,
openembedded-core and meta-yocto to the commits the `yocto-6.0.4` tags point
to, and meta-openembedded to the head of `wrynose` that day.

**Problem 1: bitbake's branch.** I first wrote `branch: "2.16"` for bitbake.
Checked before building:

```sh
git fetch https://git.openembedded.org/bitbake 2.18
git merge-base --is-ancestor 51fd94a6… FETCH_HEAD    # true: 6.0.4 is bitbake 2.18
```

The build log later confirms `BB_VERSION = "2.18.0"`.

**Problem 2: no poky repo for 6.0.** The combined `poky` repository has no
`wrynose` branch. bitbake, oe-core and meta-yocto are separate repos now; the
kas repo names `bitbake` and `openembedded-core` matter because
`oe-init-build-env` looks for `../bitbake` (seen in `scripts/oe-buildenv-internal`).

## 2. Tooling

```sh
curl -fsSL -o scripts/kas-container https://raw.githubusercontent.com/siemens/kas/5.5/kas-container
docker pull ghcr.io/siemens/kas/kas:5.5          # 1.75 GB, Debian 13 "trixie"
```

`scripts/kas` sets `KAS_WORK_DIR`, `KAS_BUILD_DIR`, `DL_DIR`, `SSTATE_DIR` to
the build disk and calls kas-container. Nothing heavy touches the internal disk.

## 3. The Rust app, tested on the host first

```sh
cd apps/can-demo
CARGO_TARGET_DIR=/tmp/can-demo-target cargo test      # 7 tests: frame layout, ids, config parsing, gap counting
CARGO_TARGET_DIR=/tmp/can-demo-target cargo clippy --all-targets
```

Once the host had a `vcan0` (created by hand with sudo; see
[can-bridge.md](can-bridge.md)), the app also ran natively against it, with
`scripts/host-can.py` as the peer:

```
can-demo 0.1.0 starting: iface=vcan0 heartbeat_id=0x101 interval=1000ms send=true listen=true
listener: first frame from id 0x101 len 8
listener: first frame from id 0x321 len 8
listener: frames total=10 errors=0 heartbeat_last_seq=3 heartbeat_missed=0 ids[0x101=4 0x321=6]
```

Bug caught in review before the first build: the watchdog helper computed
`Instant::now() - WATCHDOG_USEC/2`, which panics when the system has been up
for less than that (exactly the case right after boot). Replaced with an
`Option<Instant>` "never pinged yet".

## 4. First parse

```sh
scripts/kas shell kas/qemuarm64.yml -c 'bitbake -p && bitbake-layers show-layers'
# Parsing of 2005 .bb files complete (0 cached, 2005 parsed). 3528 targets, 119 skipped, 0 masked, 0 errors.
```

BitBake warned that the hash equivalence database sat inside the build dir
while `SSTATE_DIR` was shared; fixed with `BB_HASHSERVE_DB_DIR = "${SSTATE_DIR}"`.

## 5. Builds

Four runs of `scripts/timed-build.sh` (which wraps `scripts/kas build
kas/qemuarm64.yml` with timing and `du`). A sampler logged task number and
disk usage every 2 minutes.

| Run | Start | Wall time | Tasks | Result |
|---|---|---|---|---|
| 1 | 11-10 22:42 | 2 h 24 min | 3840 of 5761 | failed: taskhash mismatch (my mistake, below) |
| 2 | 11-11 01:07 | 2 min | (restored from sstate) | failed: `can-setup` packaging QA |
| 3 | 11-11 01:10 | ~51 min of work, then the laptop slept (power outage) for ~3 h 40 min | 4612 of 5780 | cancelled |
| 4 | 11-11 05:46 | 2 h 16 min | 5780 of 5780 | **succeeded** |

**Total build time from nothing to a bootable image: about 5.5 hours of
running builds** (sleep excluded), on this host with the build directory on
a USB SSD. A rerun with nothing changed would take minutes (everything comes
from sstate).

Where the time went (from the task timestamps of runs 3 and 4):

| Task | Duration |
|---|---|
| `rust-native` do_install (where `x.py` builds the compiler) | 48 min |
| `linux-yocto` do_compile | 36 min |
| `cargo-native` do_compile | 24 min |
| `libstd-rs` do_compile | 5 min |
| `can-demo` do_compile (our app) | 1 min |
| `git clone --mirror` of linux-yocto into `DL_DIR` | still running after 53 min when I looked (3.6 GB); runs in parallel with other tasks |

![Build output](images/build.png)

### Problem 3: "the metadata is not deterministic" (run 1)

```
ERROR: When reparsing …/linux-yocto_6.18.bb:do_fetch, the basehash value changed from 9dc23f… to 8ab5e5….
The metadata is not deterministic and this needs to be fixed.
ERROR: linux-yocto-6.18.52+git-r0 do_recipe_qa: Taskhash mismatch …
ERROR: can-demo-0.1.0-r0 do_recipe_qa: Taskhash mismatch …
```

Cause: **I edited the layer while the build was running.** I added the
watchdog fragment to the kernel `.bbappend` and a defaults file to the
can-demo recipe. kas mounts this repo live into the container, so when
BitBake re-parsed those recipes mid-build their signatures had changed.
Nothing was wrong with the metadata. Fix: none needed beyond restarting;
lesson: **don't touch layers during a build.** BitBake kept running the
already-scheduled independent tasks for two more hours (3840 tasks done),
all of which landed in sstate, so nothing was lost.

### Problem 4: `BB_DISKMON_DIRS` "ABORT" renamed (warning)

```
WARNING: The BB_DISKMON_DIRS "ABORT" action has been renamed to "HALT", update configuration
```

Fixed in the kas file. (Old tutorials still show `ABORT`.)

### Problem 5: `S` points to a missing directory (warning)

```
WARNING: can-setup-1.0-r0 do_unpack: … the directory ${UNPACKDIR}/${BP} … pointed to by the S variable doesn't exist
```

Recipes that only install loose files have no source tree. Fix:
`S = "${UNPACKDIR}"` in `can-setup` and `edge-watchdog`.

### Problem 6: template unit files not packaged (run 2)

```
ERROR: can-setup-1.0-r0 do_package: QA Issue: can-setup: Files/directories were installed but not shipped in any package:
  /usr/lib/systemd/system/vcan@.service
  /usr/lib/systemd/system/can-bitrate@.service
```

`SYSTEMD_SERVICE:${PN} = "vcan@vcan0.service"` enables the instance, but
reading `systemd.bbclass` shows why the template was left out: it finds the
directory via the template (`vcan@.service`) and then adds the *instance*
path (`vcan@vcan0.service`, which does not exist as a file) to `FILES`.
Fix: list the templates explicitly:

```bitbake
FILES:${PN} += "${systemd_system_unitdir}/vcan@.service ${systemd_system_unitdir}/can-bitrate@.service"
```

Same for `can-demo@.service`.

### Problem 7: the laptop slept (run 3)

A power outage put the laptop to sleep; kas logged `kas execution
cancelled` at task 4612. Restarted the same command; sstate skipped the
4602 tasks already done. A small supervisor script now restarts a build that
ends with "cancelled" (up to five times).

Other warnings in the logs were harmless: four upstream download servers
failed once (`quilt`, `gmp`, `ca-certificates`, `libsocketcan`) and the
fetcher fell back to the Yocto source mirror.

## 6. `update_crates`

```sh
scripts/kas shell kas/qemuarm64.yml -c 'bitbake -c update_crates can-demo'
```

Regenerated `can-demo-crates.inc` from `Cargo.lock`; the result was
byte-identical to the file I had written by hand from the lock file's
checksum (one crate: `libc 0.2.191`).

## 7. Booting and testing

Check first that the QEMU built by Yocto (10.2.0) has what we need:

```sh
scripts/kas shell kas/qemuarm64.yml -c \
  '/build/tmp/work/x86_64-linux/qemu-helper-native/1.0/recipe-sysroot-native/usr/bin/qemu-system-aarch64 -device help | grep -iE "kvaser|ctucan|i6300"'
# name "ctucan_pci", bus PCI, desc "CTU CAN PCI"
# name "kvaser_pci", bus PCI, desc "Kvaser PCICANx"
# name "i6300esb", bus PCI, desc "Intel 6300ESB"
# (and -object can-bus, can-host-socketcan)
```

(Calling the binary under `tmp/sysroots-components/` fails with a missing
`libepoxy.so.0`: that directory holds per-recipe pieces, not a complete
sysroot. runqemu uses the `qemu-helper-native` sysroot named in the image's
`qemuboot.conf`, which `rm_work` leaves alone.)

Then `scripts/qemu-smoke.sh`. It took five runs to get the test harness
right; the image itself did not change between them:

| Run | Result | What was wrong (in the test, not the image) |
|---|---|---|
| 1 | boot ok, 1/2 | runqemu refuses `.ext4.zst` without `snapshot` → added `snapshot`. Then the second shell command never echoed. |
| 2 | 1/2 | Answering the terminal's cursor-position query did not fix it. |
| (debug) | | A raw byte log with timestamps showed commands work when typed interactively, and that after login every line ends in `\r\r\n`. My marker regex accepted one `\r`. |
| 3 | 9/11 | `timeout` is not in core-image-minimal (BusyBox without that applet) → use `candump -n N -T ms` instead. |
| 4 | **11/11** | |
| 5 | **11/11**, `HOLD=420` | final run; its console is [first-boot.log](first-boot.log) |

Details worth knowing from this:
- The guest **asks the terminal for its size** (`ESC[6n`) twice: systemd
  259's getty and Poky's `/etc/profile` `resize` function, which turns echo
  off while it waits for the answer. A serial-console driver should answer
  like a terminal; the script replies `ESC[50;200R`.
- systemd 259 prints OSC 3008 "context" escape sequences around the prompt;
  prompt-matching regexes must allow escape sequences after `# `.

Boot to login prompt: **24–32 s** under TCG (x86 host emulating
Cortex-A57, 4 vCPUs, 256 MB). The rootfs is 221 MiB (34.6 MiB as
`.ext4.zst`), of which 100 MB is used; 110 packages.

![Smoke test](images/smoke-test.png)

## 8. Disk usage

| | Size |
|---|---|
| `downloads/` (DL_DIR) | 8.4 GB (3.6 GB of it the linux-yocto git mirror) |
| `sstate/` | 4.9 GB |
| `build/tmp/` | 18 GB (7.8 GB `work`, 2.8 GB `deploy`, 1.6 GB `sysroots-components`) |
| `work/` (layer checkouts) | 0.4 GB |
| **Total** | **~32 GB** after the build; **peak 54 GB used** during run 4 |

Peak `tmp/` was 42 GB: `rm_work` only deletes a recipe's work dir after that
recipe finishes, and big recipes (rust, kernel, gcc) are large while they
build. Free space never dropped below 26 GB, so the 10 GB guard never fired.

## 9. Host side: watching the bus

See [can-bridge.md](can-bridge.md). One extra problem with a host-side Rust
tool: its build vanished a few hours later. A cargo wrapper on this host
replaced `target/` with a symlink into a cache on the internal disk, and the
cache was evicted. Rebuilt with an explicit `CARGO_TARGET_DIR` on the build
disk.

---

# Part 2: read-only root and A/B updates

2026-11-11, 09:05–11:15 (WIB). Goal from [next-steps.md](next-steps.md):
read-only rootfs, RAUC A/B updates with rollback, tested in QEMU. Concepts:
[learning-guide.md chapters 15–21](learning-guide.md#part-2-read-only-root-and-ab-updates).

## 10. Research before writing anything

```sh
git ls-remote --heads https://github.com/rauc/meta-rauc            # wrynose: 38da0e58
git ls-remote --heads https://github.com/rauc/meta-rauc-community  # master, scarthgap only: no wrynose
git clone -b wrynose https://github.com/rauc/meta-rauc /tmp/rauc-ref/meta-rauc
git clone https://github.com/rauc/meta-rauc-community /tmp/rauc-ref/meta-rauc-community
```

meta-rauc-community has no wrynose branch, so it is **not** a layer here;
its `meta-rauc-qemuarm` was read as a reference (U-Boot boot script, flash
environment, `fw_env.config`). Read in oe-core: `overlayfs-etc.bbclass` and
its init template, `volatile-binds`, `read_only_rootfs_hook`, `u-boot.inc`
(`UBOOT_ENV`), `image_types_wic.bbclass`, and runqemu's drive handling.
Things found by reading, each of which would have cost a build otherwise:

| Found | Consequence |
|---|---|
| `overlayfs-etc`'s init script runs `mount -o remount,rw /` unless `OVERLAYFS_ETC_CREATE_MOUNT_DIRS = "0"` | the root would have been writable while looking read-only; set to 0 and create `/data` at build time |
| wic left oe-core in 6.0 (`wic-native`, git.yoctoproject.org/wic 0.3.1); plugin names use underscores (`bootimg_partition`) | checked the plugin list in the downloaded wic source |
| `WKS_FILE_DEPENDS_BOOTLOADERS:aarch64 = "grub-efi systemd-boot"` | two bootloaders built for nothing; set to `""` |
| runqemu attaches wic disks as virtio-**scsi** on qemuarm64 (`QB_DRIVE_TYPE = "/dev/sd"`) | the guest would see `/dev/sda`, not `/dev/vda`; `QB_DRIVE_TYPE = "/dev/vd"` |
| U-Boot 2026.01 `qemu_arm64_defconfig`: `CONFIG_ENV_IS_IN_FLASH`, `ENV_ADDR=0x4000000`, `ENV_SIZE=0x40000` | `fw_env.config`: `/dev/mtd0 0x4000000 0x40000 0x40000` (checked with `bitbake -c unpack u-boot`) |
| journald stores logs per machine id; an empty `/etc/machine-id` on a read-only `/etc` gives a new id every boot | `/etc` overlay on `/data` keeps the id, so old boots stay readable |
| `VOLATILE_LOG_DIR` is gone in 6.0 (`/var/log` → `volatile/log` is a `FILESYSTEM_PERMS_TABLES` choice that changes systemd's recipe) | replace the symlink in the image instead (`/var/log` → `/data/log`): no package rebuilds |
| systemd-conf ships `80-wired.network` (DHCP) | the guest still gets an address under slirp now that runqemu's `ip=dhcp` kernel argument is gone (U-Boot owns the command line) |

## 11. Keys, layer, tests

```sh
scripts/make-dev-keys.sh        # keys/dev/: dev CA, signing cert/key, other-ca/ (git-ignored)
scripts/kas shell kas/qemuarm64.yml -c 'bitbake -p'
# Parsing of 2024 .bb files complete (0 cached, 2024 parsed). 3549 targets, 119 skipped, 0 masked, 0 errors.
```

What was added is listed in the commit "Read-only rootfs and RAUC A/B
updates with U-Boot boot counting". The can-demo app gained a persistent
start counter (`StateDirectory=can-demo`), host tests: 8 pass.

## 12. Builds

| Run | Start | Wall time | Tasks run (of 6498) | Result |
|---|---|---|---|---|
| 5 | 09:34 | 44 min (2617 s) | 941 | **succeeded first time**; 1 warning (below) |
| 6 | 10:33 | 22 min (1320 s) | 148 | succeeded, no warnings (fixes after the first test run) |

Run with `setsid nohup scripts/supervise-build.sh build5 &` (restarts a
build that a laptop sleep cancels; now in the repo instead of only on the
build disk). New compile work in run 5: kernel 22.5 min (MTD and RAUC
fragments changed its config), U-Boot 2 min, U-Boot tools 2 min, RAUC and
rauc-native < 1 min each; the rest is images, bundles and many small
native tools wic needs. The second kernel compile (run 6) took 11 min.

Warning in run 5:

```
WARNING: linux-yocto-6.18.52+git-r0 do_kernel_configcheck: [kernel config]: This BSP contains fragments with warnings:
  the following symbols were not found in the active configuration: CONFIG_MTD_CHAR
```

`CONFIG_MTD_CHAR` no longer exists in Linux 6.18 (the MTD character device
is always built); the line was removed from `mtd-flash.cfg`.

Outputs (deploy directory):

| File | Size |
|---|---|
| `edge-image-qemuarm64.rootfs.wic` (whole disk) | 913 MiB (sparse) |
| `edge-image*-qemuarm64.rootfs.ext4` (one slot image) | 165 MiB, 106 MB used, `IMAGE_ROOTFS_EXTRA_SPACE = "0"` |
| `edge-bundle-qemuarm64.raucb` | 38 MiB (squashfs-compressed ext4 + verity tree + signature) |
| `edge-bundle-broken-qemuarm64.raucb` | 38 MiB |

Disk on the build volume afterwards: `build/tmp` 20 GB, `downloads` 8.7 GB,
`sstate` 5.7 GB, `work` 0.4 GB: this project ~35 GB, 40 GB free (the
volume also holds 3.2 GB of another project). Free space never dropped
below 40 GB during these builds, far from the 10 GB guard.

## 13. Testing

### Smoke test: passed first time

`scripts/qemu-smoke.sh` now boots the wic disk through U-Boot (runqemu reads
`QB_DEFAULT_BIOS`, `QB_DEFAULT_FSTYPE`, `QB_DRIVE_TYPE` from the image's
`qemuboot.conf`) and has four new checks (read-only root, `/data` overlays,
RAUC status, health check + mark-good). 15/15 with the CAN bridge on the
first run and again on the final build; host `vcan0` received the guest's
`0x101` heartbeats (97 and 73 frames). Boot to login: 37 s, and 92 s on the
final run while another project's Rust build loaded the host (load
average 20 on 12 cores); the A/B runs booted in 22–33 s.

The console showed the boot path working as designed:

```
edge-boot: slot A (partition 2), attempts left A=2 B=3, order A B
Kernel command line: root=/dev/vda2 rootfstype=ext4 ro rootwait console=ttyAMA0,115200 panic=10 rauc.slot=A
EXT4-fs (vda2): mounted filesystem … ro with ordered data mode.
PREINIT: Start
EXT4-fs (vda4): mounted filesystem … r/w with ordered data mode.
PREINIT: done; starting </sbin/init>
…
touch: /usr/test-write: Read-only file system
/data/overlay-etc/upper /etc overlay rw,…,lowerdir=/etc,upperdir=/data/overlay-etc/upper,…
overlay /var/lib overlay rw,…,lowerdir=/var/lib,upperdir=/data/var-lib,…
```

### A/B test, run 1: 18/23, every failure in the test

`scripts/qemu-ab.sh` on build 5. The update, the reboot into B, the
three failed boots of the broken release and the fallback all worked. The
five failures:

| Failure | Cause | Fix |
|---|---|---|
| "booted from A, A marked good" and "slot A bad" | `rauc status` colours its output; `boot status: good` was really `boot status: \e[32mgood\e[0m` | the console driver strips escape sequences from command output |
| journal of the previous boot | `tail -n 1` picked systemd's "Stopped Health check…" line, not the script's verdict | `grep '^edge-health:'` |
| failed health check on the console | after boot, systemd stops printing unit status on the console, so the failure was only in the journal | `StandardOutput=journal+console` in `edge-health.service` (a real device should say on its console why it reboots) |
| foreign-signed bundle | `rauc resign` must mount the bundle (loop device, root), impossible in the unprivileged kas container: `cannot check bundle payload without exclusive access: unable to find mounted device for bundle` | `scripts/forge-bundles.py`: a bundle ends in `[CMS signature][size]`; the CMS contains the manifest. openssl re-signs it with the other CA, and also makes a second bundle with `compatible=some-other-product` signed with *our* key |

Also fixed: the boot script printed `order ${BOOT_ORDER}` literally:
U-Boot's hush shell treated the `'…'` inside a double-quoted `echo` as
quotes and did not expand the variable.

The shutdown log also showed `[FAILED] Failed unmounting /data.` That is
expected (the `/etc` overlay still uses `/data`), and the end of the
shutdown, visible during the unmonitored reboots, shows systemd's final
stage cleaning up:

```
(sd-remount)[496]: Remounting '/data' read-only with options ''.
EXT4-fs (vda4): re-mounted 774f8ab7-… ro.
(sd-umount)[497]: Unmounting '/data'.
systemd-shutdown[1]: All filesystems unmounted.
```

After the final run, `dumpe2fs -h` on the data partition of the test disk
says `Filesystem state: clean`, `Mount count: 6` (six boots, three of them
forced reboots).

### A/B test, run 2 (build 6): 24/24 in 581 s

![A/B test](images/ab-test.png)

The serial console of the six boots ([ab-test.log](ab-test.log), excerpt
in [captures/ab-console.txt](captures/ab-console.txt)):

![Serial console during update and rollback](images/ab-rollback-console.png)

The refusals, from `rauc install`:

```
LastError: signature verification failed: Verify error: unable to get local issuer certificate
LastError: Compatible mismatch: Expected 'edge-qemuarm64' but bundle manifest has 'some-other-product'
```

After the rollback ([captures/rauc-status-rollback.txt](captures/rauc-status-rollback.txt)):

![rauc status after the rollback](images/rauc-status-rollback.png)

Timings inside the test (TCG, 4 vCPU, 512 MB): boot to login 22–33 s;
`rauc install` of a 38 MiB bundle from `/data` about 10 s (signature check at 03:58:18, slot written and activated at 03:58:28 in the log); each failed boot
of the broken release about 70 s (boot, 40 s health timeout, reboot), so the
rollback took 218 s from `reboot` to U-Boot choosing B.
