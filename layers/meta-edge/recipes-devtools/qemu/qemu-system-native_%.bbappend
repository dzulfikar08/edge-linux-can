# CTU CAN FD model: report the fault confinement state and become
# error-active when the core is enabled, as the IP core does, so the Linux
# ctucanfd driver reports ERROR-ACTIVE instead of STOPPED. runqemu boots the
# image with this recipe's qemu-system-aarch64.
# See docs/learning-guide.md, chapter 27, and docs/upstream/.
FILESEXTRAPATHS:prepend := "${THISDIR}/files:"
SRC_URI += "file://0001-hw-net-can-ctucan-report-fault-confinement-state.patch"
