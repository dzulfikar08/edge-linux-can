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
