SUMMARY = "Data partition layout, persistent journal and the boot health check"
DESCRIPTION = "Glue for a read-only rootfs with A/B updates: prepares /data, \
keeps the journal on it, and decides after each boot whether the running \
slot is healthy (which lets RAUC mark it good)."
LICENSE = "MIT"
LIC_FILES_CHKSUM = "file://${COMMON_LICENSE_DIR}/MIT;md5=0835ade698e0bcf8506ecda2f7b4f302"

SRC_URI = " \
    file://edge-data-layout \
    file://edge-data-layout.service \
    file://10-edge-journal.conf \
    file://edge-health \
    file://edge-health.service \
"
S = "${UNPACKDIR}"

inherit systemd

SYSTEMD_SERVICE:${PN} = "edge-data-layout.service edge-health.service"

do_install() {
    install -d ${D}${libexecdir} ${D}${systemd_system_unitdir}
    install -m 0755 ${UNPACKDIR}/edge-data-layout ${UNPACKDIR}/edge-health ${D}${libexecdir}/
    install -m 0644 ${UNPACKDIR}/edge-data-layout.service ${UNPACKDIR}/edge-health.service \
        ${D}${systemd_system_unitdir}/
    install -d ${D}${systemd_unitdir}/journald.conf.d
    install -m 0644 ${UNPACKDIR}/10-edge-journal.conf ${D}${systemd_unitdir}/journald.conf.d/
}

FILES:${PN} += "${systemd_unitdir}/journald.conf.d"
# rauc-mark-good runs `rauc status mark-good` once edge-health has passed.
RDEPENDS:${PN} += "rauc rauc-mark-good"
