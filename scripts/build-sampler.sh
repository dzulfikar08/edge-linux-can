#!/bin/sh
# Samples build progress every 120 s until the build log has an end marker:
# task number, disk used/free, sizes of tmp/, sstate/, downloads/, load.
# Writes "LOW DISK" when less than 10 GB are free (BB_DISKMON_DIRS stops the
# build itself at 1 GB).
: "${YOCTO_STORAGE:=/mnt/yocto}"
L=$YOCTO_STORAGE/logs; log=$L/${1:-build}.log; out=$L/${1:-build}-progress.csv
[ -f "$out" ] || echo "epoch,task,total,used_gb,free_gb,tmp_gb,sstate_gb,dl_gb,load1" > "$out"
while :; do
    t=$(grep -oE 'Running (setscene )?task [0-9]+ of [0-9]+' "$log" 2>/dev/null | tail -1 | grep -oE '[0-9]+ of [0-9]+' | sed 's/ of /,/')
    used=$(df -BG --output=used "$YOCTO_STORAGE" | tail -1 | tr -dc 0-9)
    free=$(df -BG --output=avail "$YOCTO_STORAGE" | tail -1 | tr -dc 0-9)
    tmp=$(du -s -BG "$YOCTO_STORAGE/build/tmp" 2>/dev/null | cut -f1 | tr -dc 0-9)
    ss=$(du -s -BG "$YOCTO_STORAGE/sstate" | cut -f1 | tr -dc 0-9)
    dl=$(du -s -BG "$YOCTO_STORAGE/downloads" | cut -f1 | tr -dc 0-9)
    echo "$(date +%s),${t:-0,0},$used,$free,${tmp:-0},$ss,$dl,$(cut -d' ' -f1 /proc/loadavg)" >> "$out"
    grep -q '^### end' "$log" 2>/dev/null && break
    [ "$free" -lt 10 ] && echo "LOW DISK" >> "$out"
    sleep 120
done
