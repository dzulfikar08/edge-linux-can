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

# CVE triage for CVEs without kernel.org CNA version data (old NVD entries),
# which sbom-cve-check therefore cannot place (docs/cve/README.md). Only
# entries with a checkable reason; config options are from
# work-shared/qemuarm64/kernel-build-artifacts/.config (Linux/arm64 6.18.59).
CVE_STATUS[CVE-2020-10774] = "not-applicable-platform: /proc/sys/kernel/rh_features exists only in Red Hat kernels (4.18.0-*.el8); not in linux-yocto sources"
CVE_STATUS[CVE-2023-3640] = "not-applicable-platform: x86 cpu_entry_area mapping (arch/x86 only); this kernel is arm64"
CVE_STATUS[CVE-2022-0400] = "not-applicable-config: SMC protocol stack (net/smc), CONFIG_SMC is not enabled"
CVE_STATUS[CVE-2022-1247] = "not-applicable-config: ROSE protocol (net/rose), CONFIG_ROSE is not enabled (CONFIG_HAMRADIO is not set)"
CVE_STATUS[CVE-2023-3397] = "not-applicable-config: JFS filesystem, CONFIG_JFS_FS is not set"
CVE_STATUS[CVE-2023-6238] = "not-applicable-config: NVMe host driver, CONFIG_BLK_DEV_NVME and CONFIG_NVME_TARGET are not set"
