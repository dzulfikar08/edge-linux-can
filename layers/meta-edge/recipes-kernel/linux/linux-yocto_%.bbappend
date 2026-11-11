FILESEXTRAPATHS:prepend := "${THISDIR}/files:"

# SocketCAN core, vcan, the two PCI CAN controllers QEMU can emulate,
# the i6300esb watchdog, and the flash that holds U-Boot's environment.
# (meta-rauc adds its own fragment for loop, squashfs and dm-verity.)
SRC_URI += "file://can.cfg file://watchdog.cfg file://mtd-flash.cfg"

# Newer 6.18 stable kernel than oe-core wrynose pins (6.18.54 on 2026-11-11):
# the heads of linux-yocto's v6.18/standard/base branch ("Merge tag
# 'v6.18.59'", 2026-10-26) and of yocto-kernel-cache's yocto-6.18 branch
# ("kver: bumping to v6.18.59"), the pair the linux-yocto maintainer
# integrated and the next oe-core bump would use. The kernel.org CNA lists
# 6.18.55-6.18.59 as the fix version for ~300 CVEs open in 6.18.54
# (docs/cve/README.md). Drop these three lines once oe-core reaches 6.18.59.
# (This bbappend applies to linux-yocto only.)
SRCREV_machine:qemuarm64 = "0aa9c16e54e9a318d97bae25e0082d7807186232"
SRCREV_meta = "cf1eb35154969907fb08e980fe6d1881c97d6a97"
LINUX_VERSION = "6.18.59"

