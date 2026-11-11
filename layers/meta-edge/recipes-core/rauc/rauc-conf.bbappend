# Our RAUC system configuration and the development CA as the keyring.
FILESEXTRAPATHS:prepend := "${THISDIR}/files:${EDGE_KEYS_DIR}:"
RAUC_KEYRING_FILE = "ca.cert.pem"

python () {
    import os
    ca = os.path.join(d.getVar("EDGE_KEYS_DIR"), "ca.cert.pem")
    if not os.path.exists(ca):
        bb.fatal("%s is missing: run scripts/make-dev-keys.sh first" % ca)
}
