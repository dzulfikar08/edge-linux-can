SUMMARY = "edge-image as release 1.1.0: the content of the good update bundle"
# Same packages as edge-image; only the version marker differs. In a real
# project this would be the next build of edge-image itself; a separate
# recipe lets one build produce both the installed and the update version.
require recipes-core/images/edge-image.bb
EDGE_VERSION = "1.1.0"
# Only the rootfs image is needed (it goes into edge-bundle).
IMAGE_FSTYPES = "ext4"
