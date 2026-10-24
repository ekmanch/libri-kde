#!/usr/bin/env bash
# Records, every two seconds, what KWin and Libri are doing, so a desktop
# freeze can be read back afterwards: whether KWin's event loop answers
# D-Bus, how much CPU KWin used, the GPU's load and memory, each client's
# VRAM, the load average, and every KWin and Libri thread's state (D is
# stuck in the kernel, R is spinning). A gap between timestamps longer
# than a few seconds marks a stall of the whole system. Start it before
# using Libri and leave it running; read the log once the desktop is
# back. It replaces ~/.cache/libri/freeze-watch.sh from 2026-10-23.
#
#   scripts/freeze-monitor.sh            # writes /tmp/libri-freeze-<date>.log
#   scripts/freeze-monitor.sh my.log     # or to a file of your choosing
set -u
log="${1:-/tmp/libri-freeze-$(date +%Y%m%d-%H%M%S).log}"
kwin=$(pgrep -x kwin_wayland | head -1)
echo "monitoring kwin $kwin, log $log"
echo "# $(date -Is) start; kwin=$kwin; kernel $(uname -r); nvidia $(cat /proc/driver/nvidia/version 2>/dev/null | head -1 | cut -c1-60)" >>"$log"
threads() {
    # pid: state, wchan and syscall number of every thread.
    local pid=$1 t
    for t in /proc/"$pid"/task/*; do
        [ -r "$t/stat" ] || continue
        printf '    %s %s wchan=%s syscall=%s\n' \
            "$(cut -d' ' -f3 "$t/stat" 2>/dev/null)" \
            "$(tr -d '\n' <"$t/comm" 2>/dev/null)" \
            "$(cat "$t/wchan" 2>/dev/null)" \
            "$(cut -d' ' -f1 "$t/syscall" 2>/dev/null)"
    done | sort | uniq -c
}
prev_kwin=0
while true; do
    now=$(date -Is)
    ticks=$(awk '{print $14+$15}' /proc/"$kwin"/stat 2>/dev/null || echo 0)
    busy=$((ticks - prev_kwin)); prev_kwin=$ticks
    if timeout 2 qdbus6 org.kde.KWin /KWin org.kde.KWin.currentDesktop >/dev/null 2>&1; then
        dbus=answers
    else
        dbus=NO_ANSWER
    fi
    gpu=$(timeout 2 nvidia-smi --query-gpu=utilization.gpu,memory.used --format=csv,noheader 2>/dev/null || echo "n/a")
    # VRAM per client, as "kwin_wayland:503 libri:98 …" in MiB.
    vram=$(timeout 2 nvidia-smi --query-compute-apps=process_name,used_memory --format=csv,noheader,nounits 2>/dev/null \
        | awk -F', *' '{n=$1; sub(".*/","",n); printf "%s:%s ", n, $2}')
    {
        echo "$now kwin-dbus=$dbus kwin-cpu-ticks=$busy gpu=$gpu vram[$vram] load=$(cut -d' ' -f1 /proc/loadavg)"
        echo "  kwin threads:"
        threads "$kwin"
        for pid in $(pgrep -f 'libri$' ; pgrep -f 'target/(debug|release)/libri'); do
            echo "  libri $pid threads:"
            threads "$pid"
        done
    } >>"$log"
    sleep 2
done
