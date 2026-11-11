SUMMARY = "SocketCAN heartbeat sender and frame counter (Rust, systemd service)"
LICENSE = "MIT"
LIC_FILES_CHKSUM = "file://LICENSE;md5=7b93a86180872dc11268f2d9c866370f"

# The source lives in this repository (apps/can-demo), not in a separate git repo.
FILESEXTRAPATHS:prepend := "${THISDIR}/../../../../apps:"
SRC_URI = " \
    file://can-demo/ \
    file://can-demo@.service \
    file://can-demo-can0.default \
"
S = "${UNPACKDIR}/can-demo"

# cargo: builds with the Yocto Rust toolchain, offline, from vendored crates.
# cargo-update-recipe-crates: `bitbake -c update_crates can-demo` rewrites
# can-demo-crates.inc from Cargo.lock.
inherit cargo cargo-update-recipe-crates systemd
require ${BPN}-crates.inc

SYSTEMD_SERVICE:${PN} = "can-demo@can0.service can-demo@vcan0.service"
# The template behind the instances is not added automatically (see can-setup).
FILES:${PN} += "${systemd_system_unitdir}/can-demo@.service"

do_install:append() {
    install -d ${D}${systemd_system_unitdir}
    install -m 0644 ${UNPACKDIR}/can-demo@.service ${D}${systemd_system_unitdir}/
    install -d ${D}${sysconfdir}/default
    install -m 0644 ${UNPACKDIR}/can-demo-can0.default ${D}${sysconfdir}/default/can-demo-can0
}
