#!/bin/sh
# A/B update and rollback test in QEMU (see scripts/qemu-ab.py for the steps).
# Needs: a finished `scripts/kas build kas/qemuarm64.yml` (edge-image,
# edge-bundle, edge-bundle-broken) and keys/dev from scripts/make-dev-keys.sh.
#
#   scripts/qemu-ab.sh            # exit 0 = all checks passed
#
# The serial console of all boots goes to docs/ab-test.log (LOG=... overrides
# it, relative to the repo). The test works on a copy of the disk image in
# build/tmp/ab-test/, so the deployed image is not changed.
set -eu
repo=$(cd "$(dirname "$0")/.." && pwd)
cd "$repo"
: "${LOG:=docs/ab-test.log}"
case "$LOG" in
    /*) echo "LOG must be a path inside the repository, relative to it (got $LOG)" >&2; exit 2 ;;
esac
# Two bundles the device must refuse (wrong signer, wrong compatible), made
# from the good one on the host with openssl; the container sees them in
# /build/tmp/ab-forged.
: "${YOCTO_STORAGE:=/mnt/yocto}"
deploy=$YOCTO_STORAGE/build/tmp/deploy/images/qemuarm64
python3 -I scripts/forge-bundles.py "$deploy/edge-bundle-qemuarm64.raucb" keys/dev "$YOCTO_STORAGE/build/tmp/ab-forged"
exec scripts/kas shell kas/qemuarm64.yml -c "python3 /repo/scripts/qemu-ab.py --log /repo/$LOG"
