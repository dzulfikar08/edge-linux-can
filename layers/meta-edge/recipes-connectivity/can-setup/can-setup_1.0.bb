SUMMARY = "systemd units that bring up CAN interfaces at boot"
DESCRIPTION = "vcan@.service creates a virtual CAN interface; \
can-bitrate@.service sets the bit rate of a real CAN controller and brings it up \
when the interface appears."
LICENSE = "MIT"
LIC_FILES_CHKSUM = "file://${COMMON_LICENSE_DIR}/MIT;md5=0835ade698e0bcf8506ecda2f7b4f302"

SRC_URI = "file://vcan@.service file://can-bitrate@.service"
# Only loose files, no source tree to unpack into.
S = "${UNPACKDIR}"

inherit systemd allarch

SYSTEMD_SERVICE:${PN} = "vcan@vcan0.service can-bitrate@can0.service"

# systemd.bbclass ships the unit for an instance name (vcan@vcan0.service)
# but not the template file behind it, so list the templates here.
FILES:${PN} += "${systemd_system_unitdir}/vcan@.service ${systemd_system_unitdir}/can-bitrate@.service"

RDEPENDS:${PN} = "iproute2 kmod"

do_install() {
    install -d ${D}${systemd_system_unitdir}
    install -m 0644 ${UNPACKDIR}/vcan@.service ${UNPACKDIR}/can-bitrate@.service ${D}${systemd_system_unitdir}/
}
