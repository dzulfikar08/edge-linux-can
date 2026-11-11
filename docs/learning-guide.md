# Yocto for a Rust and industrial-protocols engineer

This guide walks through this repository in the order it was built. It assumes
you know Rust, Linux on the command line and CAN as a protocol, and that you
have not used Yocto before. Each concept is explained where the project first
needs it. What actually happened during the build (commands, timings, errors
and fixes) is in [build-journal.md](build-journal.md).

Contents

1. [What Yocto is, in one page](#1-what-yocto-is-in-one-page)
2. [Releases and why this project uses 6.0 "Wrynose"](#2-releases)
3. [kas: one YAML file instead of a setup ritual](#3-kas)
4. [Layers](#4-layers)
5. [Machine, distro, image](#5-machine-distro-image)
6. [Recipes and the task pipeline](#6-recipes-and-the-task-pipeline)
7. [bbappend: changing someone else's recipe](#7-bbappend)
8. [Kernel configuration fragments](#8-kernel-configuration-fragments)
9. [Building Rust: the cargo recipe flow](#9-building-rust-the-cargo-recipe-flow)
10. [systemd units in an image](#10-systemd-units-in-an-image)
11. [Where the build puts things: DL_DIR, SSTATE_DIR, TMPDIR, rm_work](#11-where-the-build-puts-things)
12. [runqemu](#12-runqemu)
13. [CAN on Linux: SocketCAN, vcan, real controllers](#13-can-on-linux)
14. [Bridging the guest's CAN bus to the host](#14-bridging-the-guests-can-bus-to-the-host)
15. [Partitioning with wic](#15-partitioning-with-wic)
16. [A read-only root filesystem](#16-a-read-only-root-filesystem)
17. [Overlays and the data partition](#17-overlays-and-the-data-partition)
18. [The bootloader, its environment, boot counting](#18-the-bootloader-its-environment-boot-counting)
19. [RAUC: slots, bundles, signing](#19-rauc-slots-bundles-signing)
20. [Health checks and rollback](#20-health-checks-and-rollback)
21. [Testing it: scripts/qemu-ab.sh](#21-testing-it-scriptsqemu-absh)
22. [Glossary](#22-glossary)

---

## 1. What Yocto is, in one page

Yocto is not a Linux distribution. It is a set of tools and metadata for
**building your own** distribution from source, for a specific board, in a
reproducible way. You describe *what* you want (a kernel, a C library, systemd,
your application, a few config files) and it builds everything, including the
cross-compilers, and produces a bootable image.

The pieces:

| Piece | What it is |
|---|---|
| **BitBake** | The build engine. A task scheduler written in Python that reads metadata and runs tasks (fetch, compile, package...) in dependency order, many in parallel. Think `cargo` + `make` for a whole OS. |
| **OpenEmbedded-Core (oe-core)** | The core metadata: recipes for gcc, glibc, busybox, systemd, the Linux kernel, Rust, Python and ~900 more; the classes (`cargo.bbclass`, `systemd.bbclass`, ...) that recipes reuse; QEMU machine definitions. |
| **Poky** | The Yocto Project's *reference distribution* configuration (`meta-poky`, in the `meta-yocto` repo). A sane default distro policy. |
| **Layers** | Git repos (or directories) of metadata that add or change things. Your product is one or more layers on top of the above. |

Why industry uses it instead of Debian or Buildroot:
- **Reproducible, auditable builds**: every package is built from pinned source
  with recorded checksums and licences (an SBOM and licence manifest fall out of
  the build).
- **Long-term maintenance**: LTS releases get security fixes for 4 years.
- **Customisation without forking**: you never edit upstream recipes; you add
  layers that append to them.
- **Cross-compilation for any architecture** handled for you.

The price: the first build compiles a whole toolchain and OS from source and
takes hours, and the concepts take a while to sink in. That is what this guide
is for.

## 2. Releases

Yocto makes a release every six months, named after a place. Every second year
the April release is an **LTS** (4 years of support). At the time of writing
(November 2026):

| Version | Codename | Status |
|---|---|---|
| 5.0 | Scarthgap | LTS (April 2024) |
| 6.0 | **Wrynose** | **LTS (April 2026)**, point release 6.0.4 |
| 6.1 | (next) | in development (milestones M1–M3 tagged) |

How this was checked (no guessing from memory):

```sh
git ls-remote --heads https://git.openembedded.org/openembedded-core   # branches: scarthgap, ..., whinlatter, wrynose
git ls-remote --tags  https://git.openembedded.org/openembedded-core 'yocto-6.0*'
```

Every layer has a branch per release (`wrynose`), and the layer's
`conf/layer.conf` declares which releases it works with
(`LAYERSERIES_COMPAT_edge = "wrynose"`). Mixing layers from different releases
is the classic beginner mistake; BitBake refuses to parse when the compat
strings do not match.

**Change in 6.0**: the old `poky` repository (bitbake + oe-core + meta-poky in
one tree) is no longer published for new releases. You check out **bitbake**,
**openembedded-core** and **meta-yocto** separately. oe-core's
`oe-init-build-env` looks for bitbake in `../bitbake`, which is why the kas
file names the repos exactly `bitbake` and `openembedded-core`. Also, bitbake
has its own version numbers: Yocto 6.0 uses bitbake **2.18** (I first wrote
2.16 and had to correct it by checking which branch contains the tag commit).

## 3. kas

Without a tool, setting up a Yocto build means cloning N repos at the right
commits, sourcing `oe-init-build-env`, editing `conf/bblayers.conf` and
`conf/local.conf` by hand. Nothing records that state, so "works on my
machine" happens fast.

**kas** (by Siemens) replaces this with one YAML file:
[`kas/qemuarm64.yml`](../kas/qemuarm64.yml). It lists the repos with
pinned commits, which layers inside each repo to enable, `machine`, `distro`,
`target`, and extra lines for `local.conf`. `kas build file.yml` clones,
checks out, generates the conf files and runs bitbake.

**kas-container** runs kas inside the official kas Docker image (Debian 13).
That matters here because the host is Arch Linux, which Yocto does not list as
a supported build host: the build host's compilers and libraries leak into
"native" tools, so Yocto tests against specific distros. The container gives a
supported one without touching the host.

The project wraps it in [`scripts/kas`](../scripts/kas), which only sets four
environment variables so every heavy directory lands on the external build
disk:

| Variable | Here | Holds |
|---|---|---|
| `KAS_WORK_DIR` | `/mnt/yocto/work` | the cloned layer repos |
| `KAS_BUILD_DIR` | `/mnt/yocto/build` | `conf/`, `tmp/` (all build output) |
| `DL_DIR` | `/mnt/yocto/downloads` | source tarballs and git mirrors |
| `SSTATE_DIR` | `/mnt/yocto/sstate` | the shared-state cache |

Inside the container these appear as `/work`, `/build`, `/downloads`,
`/sstate`, and this repository as `/repo`. `kas build` mounts `/repo`
read-only; `kas shell` mounts it read-write (needed later for
`update_crates`, which writes a file into the layer).

Commands used:

```sh
scripts/kas build kas/qemuarm64.yml                    # full build
scripts/kas shell kas/qemuarm64.yml                    # interactive shell with bitbake on PATH
scripts/kas shell kas/qemuarm64.yml -c 'bitbake -p'    # just parse all recipes (fast sanity check)
scripts/kas --runtime-args "--network host" shell ...  # share the host network namespace (CAN bridge)
```

## 4. Layers

A layer is a directory with `conf/layer.conf` and recipes underneath. This
build uses five:

```
openembedded-core/meta      core     toolchain, glibc, systemd, kernel, rust ...
meta-yocto/meta-poky        yocto    the "poky" distro policy
meta-openembedded/meta-oe   openembedded-layer   extra packages; we need it for can-utils
layers/meta-edge            edge     ours (this repo)
bitbake                     (not a layer: the build tool)
```

`bitbake-layers show-layers` (run in `kas shell`) prints that list with
priorities. Priority decides which layer wins when two provide the same file
or recipe. `meta-edge` has priority 10, above the upstream layers' 5.

Our [`layer.conf`](../layers/meta-edge/conf/layer.conf) explained:

```bitbake
BBPATH .= ":${LAYERDIR}"                       # where bitbake searches for conf/ and classes/
BBFILES += "${LAYERDIR}/recipes-*/*/*.bb ${LAYERDIR}/recipes-*/*/*.bbappend"
BBFILE_COLLECTIONS += "edge"                   # the layer's short name
BBFILE_PATTERN_edge = "^${LAYERDIR}/"
BBFILE_PRIORITY_edge = "10"
LAYERDEPENDS_edge = "core openembedded-layer"  # fail early if meta-oe is missing
LAYERSERIES_COMPAT_edge = "wrynose"
```

Layout convention: `recipes-<category>/<package>/<package>_<version>.bb`, with
extra files in `files/` or `<package>/` next to the recipe.

**Why can-utils needs meta-oe:** oe-core keeps a deliberately small set of
recipes; `can-utils` lives in `meta-openembedded/meta-oe/recipes-extended/socketcan/`.
Finding where a recipe lives: the OpenEmbedded layer index
(layers.openembedded.org), or grep the cgit tree for the branch.

## 5. Machine, distro, image

Three independent choices, each a config file:

- **MACHINE** (`qemuarm64`): the hardware. CPU tune (aarch64), kernel
  provider, bootloader, serial console, and for QEMU machines the `QB_*`
  variables that tell `runqemu` how to start QEMU (`-machine virt`, virtio
  disk, which serial device). For a real board you would add a BSP layer
  (e.g. `meta-raspberrypi`, `meta-freescale`) and set `MACHINE` to one of its
  machines; everything else in this repo stays the same.
- **DISTRO** (`poky`): policy. C library, init system, which features are on
  (`DISTRO_FEATURES`: systemd, ipv6, wifi, ...), default package format. In 6.0
  poky already defaults to `INIT_MANAGER = "systemd"`; the kas file sets it
  explicitly anyway so the intent is visible.
- **Image** (`edge-image`): what goes on the root filesystem. An image is
  itself a recipe ([`edge-image.bb`](../layers/meta-edge/recipes-core/images/edge-image.bb)):
  it `require`s `core-image-minimal.bb` and appends packages to
  `IMAGE_INSTALL`.

`EXTRA_IMAGE_FEATURES = "allow-empty-password empty-root-password allow-root-login"`
in the kas file makes root log in without a password on the console. That is
for a development image only; in 6.0 these three replace the old
`debug-tweaks` feature.

## 6. Recipes and the task pipeline

A recipe (`.bb`) tells BitBake how to build one piece of software. BitBake
turns every recipe into a chain of **tasks**:

```
do_fetch → do_unpack → do_patch → do_configure → do_compile → do_install
        → do_package → do_package_write_rpm → (image) do_rootfs → do_image_*
```

Each task runs in the recipe's work directory
`tmp/work/<arch>/<recipe>/<version>/`. Classes (`inherit cargo`,
`inherit systemd`, `inherit autotools`) provide the task bodies so most
recipes only declare metadata. The full build of this image is **5780 tasks** (5761 before the watchdog
recipe was added).

Smallest real example in this repo,
[`can-setup_1.0.bb`](../layers/meta-edge/recipes-connectivity/can-setup/can-setup_1.0.bb):

```bitbake
LICENSE = "MIT"
LIC_FILES_CHKSUM = "file://${COMMON_LICENSE_DIR}/MIT;md5=0835ade698e0bcf8506ecda2f7b4f302"
SRC_URI = "file://vcan@.service file://can-bitrate@.service"   # fetched from files/
inherit systemd allarch                                         # allarch: no compiled code
SYSTEMD_SERVICE:${PN} = "vcan@vcan0.service can-bitrate@can0.service"
RDEPENDS:${PN} = "iproute2 kmod"                                # runtime dependencies
do_install() {
    install -d ${D}${systemd_system_unitdir}
    install -m 0644 ${UNPACKDIR}/vcan@.service ${UNPACKDIR}/can-bitrate@.service ${D}${systemd_system_unitdir}/
}
```

Things worth knowing:
- **`LIC_FILES_CHKSUM` is mandatory.** BitBake checksums the licence text so a
  silent licence change upstream breaks the build instead of slipping through.
- **`${D}`** is the fake root you install into; packaging splits it into
  packages (`${PN}`, `${PN}-dbg`, `${PN}-dev`, ...).
- **`:${PN}`** is an *override*: `SYSTEMD_SERVICE:${PN}` means "for the main
  package". Overrides use `:` since 2021 (old docs use `_`).
- **`UNPACKDIR`** is where `file://` sources land (newer releases; old
  tutorials use `${WORKDIR}` directly).
- `bitbake -e <recipe> | grep ^VAR=` shows the final value of any variable
  after all layers, appends and overrides. That is the main debugging tool.

## 7. bbappend

You never edit a recipe in someone else's layer. You write a `.bbappend` with
the same name in your layer; BitBake concatenates it onto the original.
`%` is a wildcard for the version, so this survives kernel upgrades:

[`linux-yocto_%.bbappend`](../layers/meta-edge/recipes-kernel/linux/linux-yocto_%25.bbappend)

```bitbake
FILESEXTRAPATHS:prepend := "${THISDIR}/files:"   # look for file:// sources in our files/ first
SRC_URI += "file://can.cfg"                       # add our kernel config fragment
```

`:=` evaluates immediately (so `THISDIR` is our directory, not the original
recipe's); `:prepend` puts our path before the existing ones.

## 8. Kernel configuration fragments

`linux-yocto` builds the kernel config from a base defconfig plus
**fragments**: small `.cfg` files with only the options you care about. Any
`file://*.cfg` in `SRC_URI` is merged in during `do_kernel_configme`, and
`do_kernel_configcheck` warns if an option you asked for did not end up in the
final `.config` (usually a missing dependency).

[`can.cfg`](../layers/meta-edge/recipes-kernel/linux/files/can.cfg) enables:

| Option | Why |
|---|---|
| `CONFIG_CAN` | the SocketCAN protocol family (`PF_CAN`) |
| `CONFIG_CAN_RAW` | `CAN_RAW` sockets: what candump and can-demo use |
| `CONFIG_CAN_BCM` | broadcast manager: kernel-side cyclic send / content filters |
| `CONFIG_CAN_GW` | kernel CAN gateway/router (`cangw`) |
| `CONFIG_CAN_DEV`, `CAN_NETLINK`, `CAN_CALC_BITTIMING` | driver infrastructure; lets `ip link set can0 type can bitrate 500000` compute bit timing |
| `CONFIG_CAN_VCAN` | virtual CAN interfaces |
| `CONFIG_CAN_SJA1000`, `CONFIG_CAN_KVASER_PCI` | driver for QEMU's `kvaser_pci` device (an SJA1000 controller on PCI) |
| `CONFIG_CAN_CTUCANFD`, `CONFIG_CAN_CTUCANFD_PCI` | driver for QEMU's `ctucan_pci` (CAN FD capable), kept as an alternative |

They are `=m` (modules). Modules end up in packages named
`kernel-module-<name>` with `_` turned into `-` (`kvaser_pci.ko` →
`kernel-module-kvaser-pci`), which is how the image recipe pulls them in.

In this build `do_kernel_configcheck` succeeded with no warnings, so every
option in `can.cfg` and `watchdog.cfg` reached the final `.config`; the boot
log then shows the drivers loading.

Useful commands (for reference; this build did not need them):

```sh
bitbake -c menuconfig virtual/kernel      # interactive; needs a terminal
bitbake -c diffconfig virtual/kernel      # writes a fragment of what you changed
bitbake -c kernel_configcheck -f virtual/kernel
grep CAN tmp/work/qemuarm64-poky-linux/linux-yocto/*/build/.config   # (before rm_work deletes it)
```

## 9. Building Rust: the cargo recipe flow

oe-core 6.0 ships its own Rust toolchain (1.94.1), built from source as part of
the build: `rust-native` (a host compiler), `libstd-rs` for the target, and
`cargo`. Recipes use it through classes:

- **`cargo.bbclass`**: `do_compile` runs `cargo build --release --frozen
  --offline` with the cross target (`aarch64-poky-linux-gnu`), and `do_install`
  copies the binaries to `/usr/bin`.
- **The network is off during compile.** Every crate must be fetched in
  `do_fetch` like any other source, with a checksum. Crates are listed in
  `SRC_URI` as `crate://crates.io/<name>/<version>` and BitBake's crate fetcher
  downloads them; `cargo_common` writes a `.cargo/config` that points cargo at
  the vendored copies.
- **`cargo-update-recipe-crates.bbclass`** adds a task that reads
  `Cargo.lock` and writes that list for you:

```sh
scripts/kas shell kas/qemuarm64.yml -c 'bitbake -c update_crates can-demo'
# writes layers/meta-edge/recipes-apps/can-demo/can-demo-crates.inc
```

The recipe ([`can-demo_0.1.0.bb`](../layers/meta-edge/recipes-apps/can-demo/can-demo_0.1.0.bb))
takes its source from this repository instead of a git URL:

```bitbake
FILESEXTRAPATHS:prepend := "${THISDIR}/../../../../apps:"
SRC_URI = "file://can-demo/ file://can-demo@.service file://can-demo-can0.default"
S = "${UNPACKDIR}/can-demo"
inherit cargo cargo-update-recipe-crates systemd
require ${BPN}-crates.inc
```

`file://` on a directory means BitBake checksums every file in it, so editing
the Rust code reruns the recipe. In a product you would usually point
`SRC_URI` at a git repo and tag (`git://...;protocol=https;branch=main` plus
`SRCREV`), so the image records exactly which commit it contains.

Design choice: the app talks to SocketCAN through `libc` directly (one
dependency) instead of the `socketcan` crate (about a dozen). Fewer crates
means a shorter `crates.inc` and less to audit; the cost is about 200 lines
(tests included) of mostly `unsafe` socket code in
[`src/can.rs`](../apps/can-demo/src/can.rs), with the kernel struct layouts
checked by unit tests.

## 10. systemd units in an image

`inherit systemd` plus `SYSTEMD_SERVICE:${PN}` makes the package enable those
units at image build time (it runs `systemctl preset` against the root
filesystem, so nothing has to happen on first boot). Template units work:
listing `can-demo@vcan0.service` enables an instance of `can-demo@.service`.
One catch, found the hard way ([build journal, problem 6](build-journal.md#problem-6-template-unit-files-not-packaged-run-2)):
the class does not put the template file itself into the package, so the
recipe must add it: `FILES:${PN} += "${systemd_system_unitdir}/can-demo@.service"`.

The units in this project, and why each line is there:

**`vcan@.service`** (oneshot): `modprobe vcan`, `ip link add dev %I type vcan`,
`ip link set dev %I up`. `RemainAfterExit=yes` keeps it "active" after the
commands finish so other units can order after it. Enabled for `vcan0`,
`WantedBy=multi-user.target`.

**`can-bitrate@.service`** (oneshot): a real CAN controller must have a bit
rate before it can go up. `ip link set %I type can bitrate 500000 restart-ms 100`
then `up`. It is `WantedBy=sys-subsystem-net-devices-%i.device`: systemd
creates a `.device` unit for every network interface, so this starts *when
can0 appears*, which is when the PCI driver binds. If the hardware is absent
(QEMU started without the CAN device), it simply never starts; no failed unit.

**`can-demo@.service`**: the Rust service, one instance per interface.

```ini
BindsTo=sys-subsystem-net-devices-%i.device   # stop if the interface disappears
After=... can-bitrate@%i.service vcan@%i.service
Type=notify            # the app sends READY=1 over $NOTIFY_SOCKET
WatchdogSec=10s        # and WATCHDOG=1 at least every 5 s, or systemd restarts it
DynamicUser=yes        # runs as a throwaway unprivileged user
RestrictAddressFamilies=AF_CAN AF_UNIX AF_NETLINK
CapabilityBoundingSet= # no capabilities: a CAN_RAW socket does not need any
EnvironmentFile=-/etc/default/can-demo-%i     # e.g. CAN_DEMO_ID=0x101 for can0
WantedBy=sys-subsystem-net-devices-%i.device
```

**`edge-watchdog`** (a config drop-in, not a unit): systemd itself can feed
a hardware watchdog. `/usr/lib/systemd/system.conf.d/10-watchdog.conf` sets
`RuntimeWatchdogSec=30s`: PID 1 pets `/dev/watchdog0`, and if PID 1 hangs
the hardware resets the machine. In QEMU the hardware is `-device i6300esb`
(an Intel PCI watchdog), driver `CONFIG_I6300ESB_WDT=y` from
`watchdog.cfg`. Boot log: `systemd[1]: Using hardware watchdog
/dev/watchdog0: 'i6300ESB timer'`. Two levels, then: systemd restarts a
hung *service* (`WatchdogSec=` per unit), the hardware resets a hung
*system*. (A reset was not provoked in this build.)

Logging: the app writes to stdout; systemd connects stdout to the journal and
sets `$JOURNAL_STREAM`. When that variable is present the app prefixes lines
with `<6>` / `<4>` / `<3>` (info/warning/error), which journald turns into
priorities (`journalctl -p warning` works). No logging crate needed.

## 11. Where the build puts things

| Directory | What | Size here | Safe to delete? |
|---|---|---|---|
| `DL_DIR` | source archives, git mirrors | 8.4 GB | yes, but you re-download |
| `SSTATE_DIR` | **shared state cache**: the packaged output of every task, keyed by a hash of all its inputs | 4.9 GB | yes, but you rebuild from scratch |
| `build/tmp` | work dirs, sysroots, `deploy/images` (the result) | 18 GB (peak 42 GB while building) | yes (`rm -rf tmp`); sstate makes the rebuild fast |
| `build/cache` | parsed recipe cache, hash equivalence DB | small | yes |

**sstate** is why the second build takes minutes instead of hours. Before
running a task, BitBake computes its signature (hash of the recipe, the
variables it uses and its dependencies' signatures). If an sstate object with
that signature exists, it unpacks it instead of running the task
(`Running setscene task` in the log). Share `SSTATE_DIR` (or serve it over
HTTP with `SSTATE_MIRRORS`) across builds and machines.
`BB_HASHSERVE_DB_DIR = "${SSTATE_DIR}"` keeps the *hash equivalence* database
(which lets two different signatures share output when the result is
byte-identical) next to the cache; BitBake warned about this on the first
parse.

**`INHERIT += "rm_work"`** deletes each recipe's work directory once its
output is packaged. Without it `tmp/` for this image would be tens of GB more.
The cost: you can't inspect a recipe's build tree afterwards. Exclude a recipe
with `RM_WORK_EXCLUDE += "can-demo"` while you debug it.

`BB_DISKMON_DIRS` makes BitBake stop starting new tasks when free space drops
under 1 GB, instead of failing halfway through writing a file.

## 12. runqemu

oe-core builds its own QEMU (`qemu-system-native`) and ships the `runqemu`
script, which reads the `QB_*` variables of the machine and the deploy
directory and starts the right command line. Typical use in `kas shell`:

```sh
runqemu qemuarm64 edge-image nographic slirp
```

- `nographic`: no window, serial console on your terminal (exit with `Ctrl-a x`).
- `slirp`: user-mode networking; no root needed (the default `tap` needs sudo
  to create a tap device).
- `qemuparams="..."`: extra QEMU arguments appended to the command line.
- `snapshot`: writes go to a temporary overlay and the image file stays
  untouched. Required here because the image is deployed as `.ext4.zst`
  (runqemu decompresses it to a temporary file only in snapshot mode).
- KVM does not help here: the host is x86-64 and the guest is aarch64, so QEMU
  emulates the CPU (TCG). That is why boot takes tens of seconds.

[`scripts/qemu-smoke.sh`](../scripts/qemu-smoke.sh) automates boot and test.
It runs [`qemu-smoke.py`](../scripts/qemu-smoke.py) inside `kas shell`, which
starts runqemu on a pseudo-terminal, waits for `login:`, logs in as root and
runs commands wrapped in markers so it can read each command's output and exit
code (`expect` in about 200 lines of Python). The full serial log is saved to
[`first-boot.log`](first-boot.log).

![Boot on the serial console](images/qemu-boot.png)

![Checks in the guest](images/guest-vcan.png)

## 13. CAN on Linux

**SocketCAN** puts CAN into the normal Linux network stack: each controller is
a network interface (`can0`), and applications use BSD sockets of family
`PF_CAN`. A `CAN_RAW` socket bound to `can0` reads and writes
`struct can_frame` (16 bytes: 32-bit id with flag bits, length, 8 data bytes):

```rust
// apps/can-demo/src/can.rs
#[repr(C)]
pub struct CanFrame { pub can_id: u32, pub len: u8, _pad: u8, _res0: u8, _len8_dlc: u8, pub data: [u8; 8] }
// socket(PF_CAN, SOCK_RAW, CAN_RAW); bind(sockaddr_can { ifindex }); read()/write() whole frames
```

The top bits of `can_id` are flags: `0x80000000` extended (29-bit) id,
`0x40000000` remote frame, `0x20000000` error frame.

Things that surprise people coming from a CAN adapter API:

- **Local loopback.** A frame sent on `can0` is delivered to every *other*
  socket on the same host bound to `can0` as if it came from the bus. That is
  why can-demo's listener (a second socket) counts its own heartbeats, and why
  `candump` in the guest shows them. The sending socket itself does not get it
  back unless it sets `CAN_RAW_RECV_OWN_MSGS`.
- **vcan** is a virtual interface with no bus at all: everything sent is
  looped back locally. Perfect for testing software, says nothing about bit
  timing or bus errors.
- **Real controllers need a bit rate** (`ip link set can0 type can bitrate
  500000`) before `up`, and report bus state (`ERROR-ACTIVE`,
  `ERROR-PASSIVE`, `BUS-OFF`) in `ip -details link show can0`.
  `restart-ms 100` makes the driver recover from bus-off automatically.
- **can-utils**: `candump can0` (receive), `cansend can0 123#DEADBEEF`
  (send), `cangen` (random traffic), `canbusload`, `isotpsend/isotprecv`, ...

## 14. Bridging the guest's CAN bus to the host

QEMU can emulate CAN controllers and connect them to a virtual bus:

```
-object can-bus,id=canbus0                                   # a virtual CAN bus inside QEMU
-object can-host-socketcan,id=canhost0,if=vcan0,canbus=canbus0   # bridge that bus to host vcan0
-device kvaser_pci,canbus=canbus0                            # a PCI CAN card (SJA1000) on that bus
```

The guest sees a PCI Kvaser card; the `kvaser_pci` driver binds and creates
`can0`. Frames the guest sends on `can0` appear on the host's `vcan0`, and
frames written to host `vcan0` arrive at guest `can0`. On the host you can
watch with any SocketCAN tool, or with `scripts/can-watch.py`, which decodes
the frames with [`docs/can-demo.dbc`](can-demo.dbc).

Two practical requirements:

1. **Host `vcan0` must exist.** Creating it needs root, once per boot:
   ```sh
   sudo modprobe vcan
   sudo ip link add dev vcan0 type vcan
   sudo ip link set up vcan0
   ```
2. **QEMU must see the host's network namespace.** QEMU runs inside the kas
   Docker container, which has its own network namespace with no `vcan0`.
   `scripts/kas --runtime-args "--network host" shell ...` shares the host's.

Details and results: [can-bridge.md](can-bridge.md).

![The guest's can0, bridged to host vcan0](images/guest-can0.png)

## Part 2: read-only root and A/B updates

Chapters 15 to 21 cover the second stage of the project: the image became a
real disk with two root filesystem slots, a read-only root, a data
partition, U-Boot, and RAUC for signed updates with automatic rollback. The
commands and problems are in [build-journal.md](build-journal.md#part-2-read-only-root-and-ab-updates).

The idea in one picture:

```
 QEMU flash bank 1             virtio disk /dev/vda (the wic image)
 ┌────────────────────┐        ┌──────────┬────────────┬────────────┬─────────────────────┐
 │ U-Boot environment │        │ vda1     │ vda2       │ vda3       │ vda4                │
 │  BOOT_ORDER=A B    │        │ boot     │ rootfs A   │ rootfs B   │ data                │
 │  BOOT_A_LEFT=3     │        │ FAT      │ ext4, ro   │ ext4, ro   │ ext4, read-write    │
 │  BOOT_B_LEFT=3     │        │ boot.scr │ kernel +   │ kernel +   │ /etc and /var/lib   │
 └────────────────────┘        │          │ rootfs     │ rootfs     │ overlays, journal,  │
   read and written by         │          │ 1.0.0      │ (empty)    │ RAUC status         │
   U-Boot (boot.scr) and       └──────────┴────────────┴────────────┴─────────────────────┘
   Linux (fw_setenv, RAUC)
```

1. U-Boot (QEMU's firmware) runs `boot.scr`, which reads `BOOT_ORDER`,
   picks the first slot that still has attempts left, decrements its
   counter, saves the environment and boots `/boot/Image` from that slot with
   `rauc.slot=A` (or `B`) on the kernel command line.
2. Linux mounts that slot read-only. State lives on `/data`.
3. When the system decides it is healthy, `rauc status mark-good` resets
   the slot's counter to 3.
4. `rauc install bundle.raucb` checks the bundle's signature, writes the new
   rootfs image into the *other* slot and puts that slot first in
   `BOOT_ORDER`. The running system is never modified.
5. If the new slot never becomes healthy, every boot of it uses one attempt;
   at 0, U-Boot boots the old slot again. That is the rollback.

## 15. Partitioning with wic

So far the build produced a bare ext4 file (`edge-image-qemuarm64.rootfs.ext4`),
and QEMU loaded the kernel itself (`-kernel Image`). A real device boots from
a disk with a partition table, so the image needs one.

**wic** ("OpenEmbedded Image Creator") builds partitioned disk images. Since
Yocto 6.0 it is its own project (`wic-native` recipe, git.yoctoproject.org/wic).
You describe the disk in a **kickstart file** (`.wks`) and add `wic` to
`IMAGE_FSTYPES`. Ours is
[`edge-ab.wks`](../layers/meta-edge/recipes-core/images/edge-ab.wks):

```
bootloader --ptable gpt
part --source bootimg_partition --fstype=vfat --label boot --active --align 1024 --fixed-size 16M
part / --source rootfs --fstype=ext4 --label rootfs_a --align 1024 --fixed-size 320M
part --fstype=ext4 --label rootfs_b --align 1024 --fixed-size 320M
part --fstype=ext4 --label data --align 1024 --fixed-size 256M
```

- Each `part` line is a partition, in order (`vda1` … `vda4`).
- `--source` names a **plugin** that fills it: `rootfs` copies the image's
  root filesystem; `bootimg_partition` copies the files listed in
  `IMAGE_BOOT_FILES` (here only U-Boot's `boot.scr`). A partition with no
  `--source` gets an empty filesystem of `--fstype`.
- `--fixed-size`: both slots must be the same size and large enough for any
  future rootfs image. Leave room to grow; changing partition sizes in the
  field is hard.
- `WKS_FILE = "edge-ab.wks"` in the image recipe selects it; wic looks next
  to the recipe and in each layer's `files/wic/`.

Gotcha: on aarch64 the wic class builds `grub-efi` and `systemd-boot` "in
case" a wks file needs them. Ours does not, so the image recipe sets
`WKS_FILE_DEPENDS_BOOTLOADERS = ""`.

## 16. A read-only root filesystem

`IMAGE_FEATURES += "read-only-rootfs"` makes the image work with `/`
mounted read-only. The class `rootfs-postcommands.bbclass` (read
`read_only_rootfs_hook`) changes `/etc/fstab` to `ro`, and the boot script
puts `ro` on the kernel command line, so `/` is never writable, not even
briefly.

Why bother on a device with A/B updates?

- **The slot is exactly the image.** Nothing on the device changes the
  installed software, so every device on version X has byte-identical
  software, and what was tested is what runs.
- **Power cuts** cannot corrupt a filesystem that is never written.
- It forces you to know **where state lives**, which you need anyway for
  updates: state that should survive an update must not live in the slot
  that the update replaces.

What still needs to be writable, and where it went in this project:

| Path | Why it is written | Solution here |
|---|---|---|
| `/run`, `/tmp`, `/var/volatile` | runtime files | tmpfs (oe-core default) |
| `/etc` | `machine-id`, admin changes | **overlay**: rootfs `/etc` below, `/data/overlay-etc/upper` on top (`overlayfs-etc`, chapter 17) |
| `/etc/machine-id` | identifies the installation; journal files are stored per machine id | written on the first boot into the `/etc` overlay, so it stays the same after updates |
| `/var/lib` | systemd state, DynamicUser state dirs (`can-demo` start counter) | overlay with `/data/var-lib` on top (`volatile-binds`, chapter 17) |
| `/var/log` | journal | symlink to `/data/log` (image post-process command); journald `Storage=persistent` |
| `/var/cache`, `/var/spool`, `/srv` | caches | tmpfs (`volatile-binds` default) |
| RAUC status | which bundle is in which slot | `data-directory=/data/rauc` in `system.conf` |

Test it: `touch /usr/x` must fail with "Read-only file system", and
`/proc/mounts` must show `/` with `ro`.

## 17. Overlays and the data partition

An **overlay filesystem** stacks a writable *upper* directory on a
read-only *lower* one. Reads see the upper file if it exists, else the
lower; writes always go to the upper; deleting a lower file leaves a
"whiteout" in the upper. The kernel needs `CONFIG_OVERLAY_FS`.

Two oe-core mechanisms use it here:

- **`overlayfs-etc`** (image feature): `/etc` must be ready before systemd
  starts (systemd reads `/etc` first thing). The class replaces `/sbin/init`
  with a small shell script that mounts the data partition
  (`OVERLAYFS_ETC_DEVICE = "/dev/vda4"`) on `/data`, mounts the `/etc`
  overlay, then `exec`s the real init (`/sbin/init.orig` → systemd).
  **Trap:** by default that script first runs `mount -o remount,rw /` to
  create mount points, which silently makes the "read-only" root writable.
  `OVERLAYFS_ETC_CREATE_MOUNT_DIRS = "0"` turns that off; the image creates
  `/data` at build time instead.
- **`volatile-binds`** (installed with read-only-rootfs): one small service
  per entry in `VOLATILE_BINDS`, each mounting an overlay (or a copy + bind
  mount) of a writable directory over a read-only one. The default puts
  tmpfs under `/var/lib`; the `.bbappend` here points `/var/lib` at
  `/data/var-lib` instead, so it persists.

What an overlay on persistent storage means for updates: the upper layer
outlives the slot. If the admin edited `/etc/foo.conf`, that edited copy
keeps hiding whatever `foo.conf` the next release ships. That is often what
you want (local configuration wins) and sometimes a bug (an old file hides a
needed new default). Real products decide per file, or keep configuration
outside `/etc` entirely; `overlayfs-etc` says so in its own comments
("update and migration strategy … is out of scope").

`/data` itself is never written by an update. A first boot (or a
reformatted partition) starts empty, so
[`edge-data-layout.service`](../layers/meta-edge/recipes-core/edge-system/files/edge-data-layout.service)
creates the directories at every boot, before the journal is flushed to
disk.

## 18. The bootloader, its environment, boot counting

`qemuarm64` normally boots without a bootloader: runqemu passes the kernel to
QEMU (`-kernel`). That cannot choose between two slots, so this stage adds
**U-Boot** (`u-boot` recipe, `qemu_arm64_defconfig`) as the firmware QEMU
runs (`-bios u-boot.bin`, set via `QB_DEFAULT_BIOS` in the image recipe).

Why U-Boot and not GRUB: both work with RAUC. U-Boot is what most ARM boards
ship, oe-core builds it for `qemuarm64` out of the box, and its environment
can live in QEMU's emulated flash just like on a board with SPI-NOR. GRUB on
aarch64 needs UEFI firmware (edk2/OVMF) built as well, and a GRUB environment
file on a FAT partition. meta-rauc-community's `meta-rauc-qemuarm` layer
uses the same U-Boot approach and was the reference for this one.

**The environment** is U-Boot's key/value store (`printenv`, `setenv`,
`saveenv`). `qemu_arm64_defconfig` keeps it in flash at `0x4000000`, the
second of the "virt" machine's two 64 MiB flash banks; the test scripts back
that bank with a file (`-drive if=pflash,index=1,file=…`) so values survive
reboots. Linux reaches the same bytes through MTD: the kernel fragment
`mtd-flash.cfg` enables the CFI flash driver, `/dev/mtd0` appears, and
`fw_printenv`/`fw_setenv` (libubootenv) read and write it as described in
`/etc/fw_env.config` (shipped by the `u-boot-env` package from our
`u-boot` bbappend).

**The boot script** ([`boot.cmd`](../layers/meta-edge/recipes-bsp/u-boot/files/boot.cmd),
compiled to `boot.scr` because the bbappend sets `UBOOT_ENV = "boot"`,
`UBOOT_ENV_SUFFIX = "scr"`) implements the boot counting:

```
for slot in "${BOOT_ORDER}"; do      # e.g. "B A" after an update
  first slot with BOOT_x_LEFT > 0  → decrement it, remember the partition
done
saveenv                              # store the decrement BEFORE booting
ext4load virtio 0:${root_part} ${kernel_addr_r} /boot/Image
booti ${kernel_addr_r} - ${fdtcontroladdr}
```

Storing the decrement before booting is the essential part: a kernel panic,
a hang caught by the watchdog, or a failed health check all end in a reboot
without anyone resetting the counter, so each costs one attempt.

Details worth knowing:

- The kernel lives inside the slot (`/boot/Image`, package
  `kernel-image-image`). Kernel and rootfs update together.
- The device tree comes from QEMU (it describes CPUs, RAM and the PCI
  devices on the command line); U-Boot passes it on as `${fdtcontroladdr}`.
  On a real board it would be a `.dtb` from the kernel build, also in the slot.
- U-Boot's "standard boot" scans the disks for `boot.scr`. On real hardware
  you would rather compile the logic into U-Boot's default environment
  (`CONFIG_BOOTCOMMAND`) so a corrupted boot partition cannot break it.

## 19. RAUC: slots, bundles, signing

**RAUC** (Robust Auto-Update Controller, by Pengutronix) is the update
client on the device. Its layer, **meta-rauc**, provides the `rauc` recipe,
the `bundle` class and config fragments for the kernel (loop, squashfs,
dm-verity) and BusyBox. `DISTRO_FEATURES:append = " rauc"` switches those
fragments on.

**Slots** are described in `system.conf` (ours:
[`system.conf`](../layers/meta-edge/recipes-core/rauc/files/system.conf),
installed by a `rauc-conf.bbappend`):

```ini
[system]
compatible=edge-qemuarm64   # bundles must say exactly this
bootloader=uboot                    # switch slots with fw_setenv
data-directory=/data/rauc           # status on the writable partition
[keyring]
path=ca.cert.pem                    # trust anchor for bundle signatures
[slot.rootfs.0]
device=/dev/vda2
type=ext4
bootname=A                          # U-Boot's name for it
[slot.rootfs.1]
device=/dev/vda3
type=ext4
bootname=B
```

RAUC knows the booted slot from `rauc.slot=A` on the kernel command line,
and only ever writes the other slot of the same class.

**A bundle** (`.raucb`) is a squashfs holding the images plus a
`manifest.raucm` (compatible string, version, image hashes), followed by a
CMS signature. With `RAUC_BUNDLE_FORMAT = "verity"` the signature covers a
dm-verity root hash, so the kernel verifies every block as RAUC reads it:
the bundle cannot be swapped after the check. The recipe is short
([`edge-bundle.inc`](../layers/meta-edge/recipes-core/bundles/edge-bundle.inc)):

```bitbake
inherit bundle
RAUC_BUNDLE_COMPATIBLE = "edge-qemuarm64"
RAUC_BUNDLE_FORMAT = "verity"
RAUC_BUNDLE_SLOTS = "rootfs"
RAUC_SLOT_rootfs = "edge-image-update"   # the image recipe whose rootfs goes in
RAUC_SLOT_rootfs[fstype] = "ext4"
RAUC_KEY_FILE = "${EDGE_KEYS_DIR}/signing.key.pem"
RAUC_CERT_FILE = "${EDGE_KEYS_DIR}/signing.cert.pem"
```

`bitbake edge-bundle` builds `edge-image-update`, then packs and signs
`edge-bundle-qemuarm64.raucb` in the deploy directory. Because the slot
type is `ext4` and the image is `.ext4`, RAUC writes the image to the
partition block for block.

**Signing and keys.** `scripts/make-dev-keys.sh` creates a development CA
and a signing certificate under `keys/dev/` (ignored by git; every subject
says "DEVELOPMENT ONLY"). The image gets only the CA certificate
(`/usr/lib/rauc/ca.cert.pem`); the build host holds the signing key. A
device accepts a bundle only if the signer's certificate chains to that CA.
In production:

- the signing key never sits on build machines (HSM or PKCS#11 token, or a
  separate signing step after CI);
- devices trust a production CA, never a development one; development
  devices may trust both;
- certificates expire and can be revoked, so plan how a device gets a new
  keyring (RAUC can update the keyring inside a bundle, signed by the old key).

The A/B test proves two refusals. Both bundles are made from the good one by
[`forge-bundles.py`](../scripts/forge-bundles.py) with plain openssl: a
verity bundle ends in `[CMS signature][8-byte size]`, and the CMS
structure carries the manifest, so it can be re-signed without touching
the payload.

```
signed by keys/dev/other-ca:
  LastError: signature verification failed: Verify error: unable to get local issuer certificate
signed with our key, manifest says compatible=some-other-product:
  LastError: Compatible mismatch: Expected 'edge-qemuarm64' but bundle manifest has 'some-other-product'
```

The second one matters as much as the first: a valid signature only says
who built the bundle, not which product it is for.

Three image recipes make the test possible from one build:
`edge-image` (1.0.0, the factory image in the wic disk), `edge-image-update`
(1.1.0, in `edge-bundle`) and `edge-image-broken` (1.2.0-broken, in
`edge-bundle-broken`: a systemd drop-in makes `can-demo@vcan0` exit with an
error, like an app release that crashes on start). The version is in
`/usr/lib/edge-release`.

## 20. Health checks and rollback

Booting is not proof that a release works. The device needs a rule for
"healthy", and only then marks the slot good. This uses systemd's boot
assessment pattern:

- [`edge-health.service`](../layers/meta-edge/recipes-core/edge-system/files/edge-health.service)
  is `RequiredBy=boot-complete.target` and `Before=` it. Its script waits
  up to 40 s for `can-demo@vcan0` to be active, then checks it stayed up
  without restarting.
- meta-rauc's `rauc-mark-good.service` runs **after** `boot-complete.target`
  and calls `rauc status mark-good`, which sets `BOOT_x_LEFT=3`.
- If the check fails, `boot-complete.target` is never reached, so the slot
  is not marked good, and `FailureAction=reboot` on the health service
  reboots. U-Boot has already taken one attempt. After three, it falls
  back to the other slot.

The other paths to rollback use the same counter: a kernel panic reboots
after `panic=10`, and a hang stops feeding the i6300esb watchdog
(chapter 10), which resets the machine. Neither resets the counter.

`rauc status mark-bad` exists too: a health check that is sure the release
is broken can mark it bad and reboot, falling back after one boot instead of
three. Counting attempts is the more general mechanism, because it also
covers the cases where nothing runs to make a decision.

What happens to the bad slot: it stays in the other partition with
`BOOT_A_LEFT=0`, shown by `rauc status` as `boot status: bad`. The next
`rauc install` overwrites it.

## 21. Testing it: scripts/qemu-ab.sh

[`qemu-ab.py`](../scripts/qemu-ab.py) runs the whole story in one QEMU
process (the guest's `reboot` resets the virtual machine but QEMU keeps
running with the same disk and flash files). It starts
`qemu-system-aarch64` directly instead of via runqemu, so the machine is
visible in one place: firmware, the flash file for U-Boot's environment,
the disk, the network and the watchdog. It copies the deployed disk image
first, so every run starts from the factory state. Bundles are served by a
small HTTP server in the test; inside the guest, `10.0.2.2` is the host side
of QEMU's user-mode network.

What the console shows over the six boots of a run (the `edge-boot` line is
U-Boot's script, the `edge-health` line the health check):

![Serial console during update and rollback](images/ab-rollback-console.png)

Read it from the top: slot A boots and is healthy; after `rauc install` the
order is `B A` and B boots with 1.1.0; the broken release goes into A,
which boots three times (`A=2`, `A=1`, `A=0`) and is unhealthy each time;
then U-Boot skips A and boots B again.

The serial log of a full run is [`ab-test.log`](ab-test.log); all 24
checks and the timings are in
[build-journal.md](build-journal.md#ab-test-run-2-build-6-2424-in-581-s).

## Lessons from this build

Each of these cost time during the build ([build-journal.md](build-journal.md)):

1. **Don't edit layers while BitBake runs.** It re-parses recipes during the
   build; a changed recipe gives "basehash value changed … metadata is not
   deterministic" and fails the build.
2. **Look up versions, don't remember them.** bitbake 2.18 goes with Yocto
   6.0, not 2.16; `git merge-base --is-ancestor <tag-commit> <branch>` settles it.
3. **Read the class when packaging surprises you.** `systemd.bbclass` is
   ~300 lines of readable Python; the template-unit `FILES` problem was
   obvious once read.
4. **Upstream renames things.** `debug-tweaks` → three explicit image
   features; `BB_DISKMON_DIRS` `ABORT` → `HALT`; `WORKDIR` → `UNPACKDIR` for
   sources. Warnings tell you; fix them, as they become errors later.
5. **A minimal image is minimal.** No `timeout`, BusyBox `head` wants `-n 3`,
   `free -m` is ignored. Test scripts must use what the image has.
6. **Serial consoles talk back.** The guest queries the terminal size and
   waits; an automated console must answer like a terminal.
7. **sstate is the safety net.** A failed or interrupted build loses
   almost nothing: rerun the same command.

From part 2 (read-only root and A/B updates):

8. **Read the class before using the feature.** `overlayfs-etc` quietly
   remounts `/` read-write unless told not to; reading 90 lines of
   `overlayfs-etc.bbclass` and its template found it before any build.
9. **Defaults are tuned for the old setup.** runqemu attached the new wic
   disk as SCSI (`/dev/sda`), wic pulled in two unused bootloaders, and
   runqemu's `ip=dhcp` vanished once U-Boot owned the kernel command line.
   Each default made sense for "QEMU boots the kernel directly".
10. **Test the failure path on purpose.** The rollback only counts as
    working because a deliberately broken release was installed and the
    device was watched falling back. A test that only installs good
    updates proves half of it.
11. **Test scripts fail more than images.** All five failures of the first
    A/B run were in the test (ANSI colours, the wrong log line, a tool that
    needs root); the image was right. Read the failure before changing
    the image.
12. **Machine identity is state.** With a plain read-only `/etc` and an
    empty `/etc/machine-id`, systemd uses a new transient machine id every
    boot, and the journal (stored per machine id) would then hide earlier
    boots. This was avoided by design (the `/etc` overlay keeps the id),
    and the A/B test checks the id is the same after the update.

## 22. Glossary

| Term | Meaning |
|---|---|
| BitBake | the task executor |
| recipe (`.bb`) | instructions to build one piece of software |
| `.bbappend` | additions to a recipe from another layer |
| `.bbclass` | reusable recipe logic, pulled in with `inherit` |
| `.conf` | configuration: `local.conf`, `bblayers.conf`, machine and distro files |
| `.inc` | a fragment included with `require`/`include` |
| layer | directory of metadata with `conf/layer.conf` |
| MACHINE / DISTRO / image | hardware / policy / root filesystem contents |
| `PN`, `PV`, `PR` | package name, version, revision (from the file name `name_version.bb`) |
| `S`, `B`, `D` | source dir, build dir, install destination |
| `DEPENDS` / `RDEPENDS` | build-time / run-time dependencies |
| sstate | shared-state cache of task outputs |
| setscene | a task restored from sstate instead of run |
| native / nativesdk / target | built for the build host / for an SDK host / for the device |
| kas | YAML front-end that sets up layers and runs bitbake |
| runqemu | script that boots a built image in QEMU |
| wic / `.wks` | tool and kickstart file that build a partitioned disk image |
| slot | one of the places a component (here the rootfs) can be installed: A or B |
| bundle (`.raucb`) | signed RAUC update file: images + manifest + signature |
| `compatible` | string a bundle must match to be installed on a device |
| boot counting | bootloader decrements a counter per boot; only a healthy system resets it |
| mark-good / mark-bad | RAUC commands that tell the bootloader a slot works / does not |
| overlayfs | writable upper directory stacked on a read-only lower one |
| U-Boot environment | the bootloader's persistent variables, shared with Linux via `fw_setenv` |
