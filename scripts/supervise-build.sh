#!/bin/sh
# Runs scripts/timed-build.sh and starts it again when kas reports
# "kas execution cancelled" (the laptop slept or lost power), up to 5 times.
# sstate keeps all finished work, so a restart only redoes unfinished tasks.
# Progress (task number, disk usage) is sampled every 2 minutes by
# scripts/build-sampler.sh into $YOCTO_STORAGE/logs/<name>-progress.csv.
#   setsid nohup scripts/supervise-build.sh build5 >/dev/null 2>&1 &
set -u
here=$(cd "$(dirname "$0")" && pwd)
: "${YOCTO_STORAGE:=/mnt/yocto}"
L=$YOCTO_STORAGE/logs
mkdir -p "$L"
base=${1:-build}; n=0
while [ $n -lt 5 ]; do
    name=$base; [ $n -gt 0 ] && name="$base-retry$n"
    "$here/build-sampler.sh" "$name" >/dev/null 2>&1 &
    "$here/timed-build.sh" "$name"; rc=$?
    echo "$(date '+%F %T') $name exit=$rc" >> "$L/supervise.log"
    [ $rc -eq 0 ] && { echo "$base DONE" >> "$L/supervise.log"; exit 0; }
    grep -q 'kas execution cancelled' "$L/$name.log" || { echo "$base FAILED" >> "$L/supervise.log"; exit $rc; }
    n=$((n + 1))
done
echo "$base GAVE UP" >> "$L/supervise.log"
exit 1
