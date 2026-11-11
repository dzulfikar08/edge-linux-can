#!/bin/sh
# Boot edge-image in QEMU and check it. Exit status 0 = all checks passed.
#
#   scripts/qemu-smoke.sh            # bridge guest can0 to host vcan0 if it is up
#   CAN_HOST=none scripts/qemu-smoke.sh   # guest-only checks (vcan0 inside the guest)
#   HOLD=300 scripts/qemu-smoke.sh        # keep the guest up 5 minutes after the checks
#
# The serial console log goes to docs/first-boot.log (LOG=... overrides it; the
# path must be relative to the repo).
# With a host CAN interface, the kas container shares the host network
# namespace (--network host) so QEMU's can-host-socketcan backend can open it,
# and scripts/host-can.py talks to the guest from the host side.
set -eu
repo=$(cd "$(dirname "$0")/.." && pwd)
cd "$repo"
: "${LOG:=docs/first-boot.log}"
: "${CAN_HOST:=vcan0}"
: "${HOLD:=0}"   # seconds to keep the guest up after the checks (e.g. to watch it from the host)

# The log is written from inside the container, where only this repo is mounted (/repo).
case "$LOG" in
    /*) echo "LOG must be a path inside the repository, relative to it (got $LOG)" >&2; exit 2 ;;
esac

can_args=""
host_pid=""
host_out=$(mktemp /tmp/host-can.XXXXXX.json)
if [ "$CAN_HOST" != none ] && ip link show "$CAN_HOST" 2>/dev/null | grep -q ',UP'; then
    echo "host $CAN_HOST is up: bridging it to guest can0"
    can_args="--can-host $CAN_HOST"
    python3 scripts/host-can.py --iface "$CAN_HOST" --send-id 321 --out "$host_out" &
    host_pid=$!
else
    echo "no host CAN interface: guest-only checks"
fi

set +e
scripts/kas --runtime-args "--network host" shell kas/qemuarm64.yml \
    -c "python3 /repo/scripts/qemu-smoke.py --log /repo/$LOG --hold $HOLD $can_args"
rc=$?
set -e

if [ -n "$host_pid" ]; then
    kill "$host_pid" 2>/dev/null || true
    wait "$host_pid" 2>/dev/null || true
    echo "host side ($CAN_HOST):"
    cat "$host_out"
    # The guest's can-demo@can0 sends heartbeats with id 0x101.
    if python3 -c 'import json,sys; d=json.load(open(sys.argv[1])); sys.exit(0 if d["received"].get("101",0)>0 else 1)' "$host_out"; then
        echo "[PASS] host $CAN_HOST received guest heartbeats (id 101)"
    else
        echo "[FAIL] host $CAN_HOST saw no guest heartbeats"
        rc=1
    fi
fi
rm -f "$host_out"
exit $rc
