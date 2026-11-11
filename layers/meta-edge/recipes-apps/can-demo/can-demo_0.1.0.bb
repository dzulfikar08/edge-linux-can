SUMMARY = "SocketCAN heartbeat and CAN FD telemetry sender, frame counter (Rust, systemd service)"
LICENSE = "MIT"
LIC_FILES_CHKSUM = "file://LICENSE;md5=7b93a86180872dc11268f2d9c866370f"

# The source is apps/can-demo in this repository, fetched with git from
# GitHub at a fixed commit (SRCREV) rather than copied from the working tree
# (file://). So the image records exactly which app commit it contains
# (package version 0.1.0+git<rev>, the SPDX SBOM, buildhistory), uncommitted
# edits never leak into a build, and the same SRCREV gives the same source on
# any machine. (A private fork can use its local clone instead:
# git://${EDGE_REPO_DIR};protocol=file;branch=main;subpath=apps/can-demo)
#
# Bumping the app: commit the change under apps/can-demo, then set SRCREV to
# that commit (git rev-parse HEAD) in a second commit; if Cargo.lock changed,
# run `bitbake -c update_crates can-demo` and commit can-demo-crates.inc too.
SRC_URI = " \
    git://github.com/dzulfikar08/edge-linux-can.git;protocol=https;branch=main;subpath=apps/can-demo \
    file://can-demo@.service \
    file://can-demo-can0.default \
"
# "can-demo: CAN FD telemetry next to the classic heartbeat"
SRCREV = "da2515c14737b8865b53d61648b4666a6e90f8ad"
# "+git" makes bitbake append the commit: 0.1.0+git0+da2515c147
PV = "0.1.0+git"
# subpath= unpacks apps/can-demo into ${UNPACKDIR}/can-demo.
S = "${UNPACKDIR}/can-demo"

# cargo: builds with the Yocto Rust toolchain, offline, from vendored crates.
# ptest-cargo (inherits cargo): with "ptest" in DISTRO_FEATURES, also builds
# the crate's `cargo test` binaries for the target and packages them with a
# run-ptest script in can-demo-ptest (run on the device by ptest-runner).
# cargo-update-recipe-crates: `bitbake -c update_crates can-demo` rewrites
# can-demo-crates.inc from Cargo.lock.
inherit ptest-cargo cargo-update-recipe-crates systemd
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
