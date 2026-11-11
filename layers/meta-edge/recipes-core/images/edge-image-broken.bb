SUMMARY = "A deliberately broken release (can-demo cannot start), for rollback tests"
require recipes-core/images/edge-image.bb
EDGE_VERSION = "1.2.0-broken"
IMAGE_FSTYPES = "ext4"
IMAGE_INSTALL:append = " edge-fault-inject"
