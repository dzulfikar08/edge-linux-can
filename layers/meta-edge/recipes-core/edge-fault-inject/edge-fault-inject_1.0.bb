SUMMARY = "Simulated bad release: makes can-demo@vcan0 fail at start (test only)"
DESCRIPTION = "Only installed in edge-image-broken. A systemd drop-in replaces \
can-demo's command with one that exits with an error, like a new app \
version that crashes on start. The health check then fails and the device \
must roll back."
LICENSE = "MIT"
LIC_FILES_CHKSUM = "file://${COMMON_LICENSE_DIR}/MIT;md5=0835ade698e0bcf8506ecda2f7b4f302"

SRC_URI = "file://50-simulated-fault.conf"
S = "${UNPACKDIR}"

do_install() {
    install -d ${D}${systemd_system_unitdir}/can-demo@vcan0.service.d
    install -m 0644 ${UNPACKDIR}/50-simulated-fault.conf ${D}${systemd_system_unitdir}/can-demo@vcan0.service.d/
}
FILES:${PN} = "${systemd_system_unitdir}/can-demo@vcan0.service.d"
