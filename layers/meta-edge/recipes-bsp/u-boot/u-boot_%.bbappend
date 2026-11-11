# U-Boot for qemuarm64 (oe-core builds qemu_arm64_defconfig) with a boot script
# that chooses rootfs slot A or B the way RAUC expects.
#
# How the pieces fit:
#  - QEMU loads u-boot.bin as firmware (-bios). QEMU's "virt" machine has two
#    64 MiB flash banks; U-Boot keeps its environment in the second one
#    (offset 0x4000000, see CONFIG_ENV_* in qemu_arm64_defconfig). The test
#    scripts back that bank with a file, so the environment survives reboots.
#  - boot.scr (compiled from boot.cmd) sits on the small FAT partition 1.
#    U-Boot's standard boot finds it there, runs it, and the script loads
#    /boot/Image from the chosen rootfs partition and boots it.
#  - Linux reaches the same environment through MTD (/dev/mtd0) with
#    fw_printenv/fw_setenv (libubootenv), configured by fw_env.config.
#    RAUC uses those tools to switch slots and to mark slots good or bad.

FILESEXTRAPATHS:prepend := "${THISDIR}/files:"

SRC_URI += " \
    file://boot.cmd \
    file://fw_env.config \
"

# Compile boot.cmd into a U-Boot script image (boot.scr) and deploy it, so wic
# can put it on the boot partition (IMAGE_BOOT_FILES in edge-image.bb).
UBOOT_ENV = "boot"
UBOOT_ENV_SUFFIX = "scr"

# Hardening: no video stack, so no BMP decoding (see files/no-bmp-rle8.cfg).
SRC_URI += "file://no-bmp-rle8.cfg"

# CVE triage (docs/cve/README.md). U-Boot 2026.01, qemu_arm64_defconfig plus
# the fragment above. Each entry names the option that keeps the vulnerable
# code out of this build; check with `bitbake -c configure u-boot` and
# grep build/.config. Revisit if the defconfig or the fragments change.
CVE_STATUS[CVE-2026-71971] = "not-applicable-config: __net_defragment() in net/net.c is only built with CONFIG_IP_DEFRAG, which is not set"
CVE_STATUS[CVE-2026-74220] = "not-applicable-config: NFS client (net/nfs.c) is only built with CONFIG_CMD_NFS, which is not set"
CVE_STATUS[CVE-2026-74221] = "not-applicable-config: NFS client (net/nfs.c) is only built with CONFIG_CMD_NFS, which is not set"
CVE_STATUS[CVE-2026-74222] = "not-applicable-config: lwIP wget (net/lwip/) is only built with CONFIG_NET_LWIP, which is not set (the legacy network stack is used)"
CVE_STATUS[CVE-2026-74225] = "not-applicable-config: DHCPv6 (net/dhcpv6.c) is only built with CONFIG_CMD_DHCP6, which needs CONFIG_IPV6; neither is set"
CVE_STATUS[CVE-2026-71972] = "not-applicable-config: files/no-bmp-rle8.cfg disables CONFIG_VIDEO, so drivers/video (video_bmp.c with video_display_rle8_bitmap()) is not built; no video_bmp/rle8 symbols in the u-boot ELF"
CVE_STATUS[CVE-2026-71973] = "not-applicable-config: SquashFS (fs/squashfs/) is only built with CONFIG_FS_SQUASHFS, which is not set"
CVE_STATUS[CVE-2026-71974] = "not-applicable-config: read_slotted_partition() is in boot/bootmeth_android.c, only built with CONFIG_BOOTMETH_ANDROID, which is not set"
