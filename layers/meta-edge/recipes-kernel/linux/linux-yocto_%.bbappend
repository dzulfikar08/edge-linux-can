FILESEXTRAPATHS:prepend := "${THISDIR}/files:"

# SocketCAN core, vcan, the two PCI CAN controllers QEMU can emulate,
# the i6300esb watchdog, and the flash that holds U-Boot's environment.
# (meta-rauc adds its own fragment for loop, squashfs and dm-verity.)
SRC_URI += "file://can.cfg file://watchdog.cfg file://mtd-flash.cfg"
