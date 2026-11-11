#!/bin/sh
# Runs a kas build with start/end time, duration and disk usage logged to
# $YOCTO_STORAGE/logs/<name>.log. Meant to be started detached:
#   setsid nohup scripts/timed-build.sh build1 >/dev/null 2>&1 &
set -u
here=$(cd "$(dirname "$0")" && pwd)
: "${YOCTO_STORAGE:=/mnt/yocto}"
name=${1:-build}; shift 2>/dev/null || true
log="$YOCTO_STORAGE/logs/$name.log"
mkdir -p "$YOCTO_STORAGE/logs"
start=$(date +%s)
echo "### start $(date '+%F %T')" > "$log"
"$here/kas" build "$here/../kas/qemuarm64.yml" "$@" >> "$log" 2>&1
rc=$?
end=$(date +%s)
{
  echo "### end $(date '+%F %T') exit=$rc duration=$((end - start))s"
  du -sh "$YOCTO_STORAGE"/work "$YOCTO_STORAGE"/build "$YOCTO_STORAGE"/downloads "$YOCTO_STORAGE"/sstate 2>/dev/null
  df -h "$YOCTO_STORAGE" | tail -1
} >> "$log"
exit $rc
