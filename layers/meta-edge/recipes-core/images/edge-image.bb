SUMMARY = "Minimal systemd image for CAN edge experiments, A/B updatable"
DESCRIPTION = "core-image-minimal plus SocketCAN tools, CAN kernel modules, \
interface setup units and the can-demo Rust service. Read-only root \
filesystem, RAUC A/B updates through U-Boot, state on a data partition."
LICENSE = "MIT"

require recipes-core/images/core-image-minimal.bb

# Version of the software in this image. Written to /usr/lib/edge-release;
# edge-image-update and edge-image-broken set other values, so the tests can
# see which version is running.
EDGE_VERSION ?= "1.0.0"

IMAGE_INSTALL:append = " \
    can-utils \
    iproute2 \
    can-setup \
    can-demo \
    edge-watchdog \
    edge-system \
    rauc \
    u-boot-env \
    kernel-module-can-raw \
    kernel-module-can-bcm \
    kernel-module-can-gw \
    kernel-module-vcan \
    kernel-module-kvaser-pci \
    kernel-module-ctucanfd-pci \
"

# --- Read-only root filesystem -------------------------------------------
# The rootfs is mounted read-only ("ro" on the kernel command line, and
# read_only_rootfs_hook sets "ro" in /etc/fstab). What still has to be
# writable goes elsewhere:
#   /etc      overlay: rootfs /etc below, /data/overlay-etc/upper on top
#             (overlayfs-etc, mounted by a small /sbin/init script before systemd)
#   /var/lib  overlay with /data/var-lib on top (volatile-binds.bbappend)
#   /var/log  symlink to /data/log (persistent journal)
#   /var/cache, /var/spool, /srv, /tmp, /run: tmpfs, empty at every boot
#   /etc/machine-id: generated on the first boot, stored in the /etc overlay
IMAGE_FEATURES += "read-only-rootfs overlayfs-etc"
OVERLAYFS_ETC_MOUNT_POINT = "/data"
OVERLAYFS_ETC_DEVICE = "/dev/vda4"
OVERLAYFS_ETC_FSTYPE = "ext4"
OVERLAYFS_ETC_MOUNT_OPTIONS = "defaults,noatime"
# The default (1) makes the init script run "mount -o remount,rw /" to create
# mount point directories, which would quietly undo the read-only root. The
# directories are created at build time instead (edge_rootfs_layout).
OVERLAYFS_ETC_CREATE_MOUNT_DIRS = "0"

ROOTFS_POSTPROCESS_COMMAND += "edge_rootfs_layout; edge_write_release;"

edge_rootfs_layout() {
    install -d ${IMAGE_ROOTFS}/data
    # /var/log is a symlink into tmpfs (/var/volatile/log) by default.
    rm -rf ${IMAGE_ROOTFS}${localstatedir}/log
    ln -s /data/log ${IMAGE_ROOTFS}${localstatedir}/log
}

edge_write_release() {
    printf 'NAME=edge-linux\nVERSION=%s\nIMAGE=%s\n' "${EDGE_VERSION}" "${PN}" \
        > ${IMAGE_ROOTFS}${nonarch_libdir}/edge-release
}

# Read-only and not extended at runtime: no spare space needed in the image.
IMAGE_ROOTFS_EXTRA_SPACE = "0"

# --- Disk image -----------------------------------------------------------
# ext4: the rootfs alone (what an update bundle carries).
# wic:  the whole disk (boot partition, rootfs A, empty rootfs B, data),
#       laid out by edge-ab.wks next to this file.
IMAGE_FSTYPES = "ext4 wic"
WKS_FILE = "edge-ab.wks"
# U-Boot's boot script goes on the FAT boot partition.
IMAGE_BOOT_FILES = "boot.scr"
EXTRA_IMAGEDEPENDS += "u-boot"
# wic on aarch64 builds grub-efi and systemd-boot by default "in case"; this
# disk uses neither.
WKS_FILE_DEPENDS_BOOTLOADERS = ""

# --- runqemu ---------------------------------------------------------------
# Boot u-boot.bin as firmware and the wic disk as a virtio disk (/dev/vda),
# instead of handing QEMU a kernel and an ext4 rootfs.
QB_DEFAULT_BIOS = "u-boot.bin"
QB_DEFAULT_FSTYPE = "wic"
# A wic image is attached as a whole disk; qemuarm64 defaults to virtio-scsi
# (/dev/sda). Use a virtio block device so the partitions are /dev/vdaN, as
# in boot.cmd, system.conf and OVERLAYFS_ETC_DEVICE.
QB_DRIVE_TYPE = "/dev/vd"
QB_MEM = "-m 512"
