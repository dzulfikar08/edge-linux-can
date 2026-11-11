SUMMARY = "Let systemd drive the hardware watchdog"
LICENSE = "MIT"
LIC_FILES_CHKSUM = "file://${COMMON_LICENSE_DIR}/MIT;md5=0835ade698e0bcf8506ecda2f7b4f302"

SRC_URI = "file://10-watchdog.conf"
# Only loose files, no source tree to unpack into.
S = "${UNPACKDIR}"

inherit allarch

do_install() {
    install -d ${D}${systemd_unitdir}/system.conf.d
    install -m 0644 ${UNPACKDIR}/10-watchdog.conf ${D}${systemd_unitdir}/system.conf.d/
}

FILES:${PN} = "${systemd_unitdir}/system.conf.d"
RDEPENDS:${PN} = "systemd"
