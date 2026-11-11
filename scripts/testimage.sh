#!/bin/sh
# oe-core runtime tests (testimage) on edge-image-test in QEMU.
#
#   scripts/testimage.sh                 # with the CAN FD bridge if host vcan0 is up
#   CAN_HOST=none scripts/testimage.sh   # guest-only
#
# bitbake boots the image with runqemu (U-Boot, wic disk, snapshot mode,
# user-mode network), logs in over ssh and runs TEST_SUITES from
# kas/qemuarm64.yml: oe-core's ping/ssh/date/systemd/ptest/parselogs cases
# and our edge_can case (layers/meta-edge/lib/oeqa/runtime/cases/).
# With host vcan0 up, kas/testimage-can.conf adds a CTU CAN FD controller
# bridged to it (the container then shares the host network namespace).
# The task log is copied to $YOCTO_STORAGE/logs/testimage-<time>.log.
set -eu
repo=$(cd "$(dirname "$0")/.." && pwd)
cd "$repo"
: "${CAN_HOST:=vcan0}"
: "${YOCTO_STORAGE:=/mnt/yocto}"
extra=""
if [ "$CAN_HOST" = vcan0 ] && ip link show vcan0 2>/dev/null | grep -q ',UP'; then
    echo "host vcan0 is up: guest can0 (ctucan_pci) bridged to it"
    extra="-R /repo/kas/testimage-can.conf"
else
    echo "no CAN bridge: guest-only CAN checks"
fi
set +e
scripts/kas --runtime-args "--network host" shell kas/qemuarm64.yml \
    -c "bitbake $extra -c testimage edge-image-test"
rc=$?
set -e
log=$(ls -t "$YOCTO_STORAGE"/build/tmp/work/qemuarm64-poky-linux/edge-image-test/*/temp/log.do_testimage 2>/dev/null | head -n 1)
if [ -n "$log" ]; then
    mkdir -p "$YOCTO_STORAGE/logs"
    cp "$log" "$YOCTO_STORAGE/logs/testimage-$(date +%Y%m%d-%H%M%S).log"
    grep -E "RESULTS|^RESULTS - |Ran [0-9]+ tests|OK - All required tests passed|FAILED -|ERROR -" "$log" | tail -n 40
fi
exit $rc
