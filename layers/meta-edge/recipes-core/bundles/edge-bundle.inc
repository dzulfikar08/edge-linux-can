# Common settings for the RAUC update bundles (meta-rauc's bundle class).
inherit bundle

LICENSE = "MIT"
LIC_FILES_CHKSUM = "file://${COMMON_LICENSE_DIR}/MIT;md5=0835ade698e0bcf8506ecda2f7b4f302"

# Must equal "compatible" in the device's /usr/lib/rauc/system.conf.
RAUC_BUNDLE_COMPATIBLE = "edge-qemuarm64"
# verity: the bundle is a squashfs with a dm-verity hash tree; RAUC checks the
# signature over the root hash, then the kernel checks every block it reads.
RAUC_BUNDLE_FORMAT = "verity"

# One image for the slot class "rootfs" (both rootfs.0 and rootfs.1 are of
# that class; RAUC writes the one that is not running).
RAUC_BUNDLE_SLOTS = "rootfs"
RAUC_SLOT_rootfs[fstype] = "ext4"

# Development signing key from scripts/make-dev-keys.sh (not committed).
RAUC_KEY_FILE = "${EDGE_KEYS_DIR}/signing.key.pem"
RAUC_CERT_FILE = "${EDGE_KEYS_DIR}/signing.cert.pem"

python () {
    import os
    for v in ("RAUC_KEY_FILE", "RAUC_CERT_FILE"):
        if not os.path.exists(d.getVar(v)):
            bb.fatal("%s is missing: run scripts/make-dev-keys.sh first" % d.getVar(v))
}
