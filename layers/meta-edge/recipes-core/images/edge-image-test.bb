SUMMARY = "edge-image plus on-target test tools, for ptest and testimage"
DESCRIPTION = "The product image (edge-image) with what the automated tests \
need on the device: ptest-runner and can-demo's unit tests (can-demo-ptest), \
and an ssh server for oe-core's testimage. Kept separate so the product \
image and its update bundles do not carry test code or an ssh server."
# Same packages, layout and boot path as the product (U-Boot, wic A/B disk,
# read-only root), so what passes here is the product's software.
require recipes-core/images/edge-image.bb
EDGE_VERSION = "1.0.0-test"

IMAGE_INSTALL:append = " ptest-runner can-demo-ptest"
# Dropbear (small) for testimage, which runs its commands over ssh. Root
# logs in without a password only because this is a development image
# (EXTRA_IMAGE_FEATURES in kas/qemuarm64.yml).
IMAGE_FEATURES += "ssh-server-dropbear"
