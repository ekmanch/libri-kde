#!/usr/bin/env bash
# Records, every two seconds, what KWin and Libri are doing, so a desktop
# freeze can be read back afterwards: whether KWin's event loop answers
# D-Bus, how much CPU KWin used, the GPU's load and memory, each client's
# VRAM, the load average, and every KWin and Libri thread's state (D is
# stuck in the kernel, R is spinning) with the kernel function it waits
# in. A gap between timestamps longer than a few seconds marks a stall of
# the whole system.
#
# Libri starts this itself (`app::run`) in its own process group, so a
# Ctrl+C in the terminal does not take it down with Libri, and the monitor
# keeps sampling for a while after Libri is gone: the 2026-10-24 21:42
# freeze began the moment Libri's window was closed, so the minutes after
# exit are the ones to record. The log is under ~/.cache/libri/, which
# survives a reboot (/tmp does not). Run it by hand to watch the desktop
# without Libri, for example to see whether the freeze happens without it:
#
#   scripts/freeze-monitor.sh                 # ~/.cache/libri/freeze-<date>.log, until Ctrl+C
#   scripts/freeze-monitor.sh my.log          # to a file of your choosing
#   scripts/freeze-monitor.sh my.log <pid>    # until 90 s after that process exits (Libri passes its own)
#   scripts/freeze-monitor.sh my.log <pid> 10 # until 10 s after it exits
set -u
dir="${XDG_CACHE_HOME:-$HOME/.cache}/libri"
mkdir -p "$dir"
log="${1:-$dir/freeze-$(date +%Y%m%d-%H%M%S).log}"
follow="${2:-}"
after="${3:-90}"
gone_at=""
kwin=$(pgrep -x kwin_wayland | head -1)
echo "monitoring kwin $kwin${follow:+, libri $follow}, log $log"
echo "# $(date -Is) start; kwin=$kwin; libri=${follow:-any}; kernel $(uname -r); nvidia $(cat /proc/driver/nvidia/version 2>/dev/null | head -1 | cut -c1-60)" >>"$log"
threads() {
    # pid: state, name and wchan of every thread. (Not the syscall number:
    # reading it needs ptrace access and the kernel logs every attempt.)
    local pid=$1 t
    for t in /proc/"$pid"/task/*; do
        [ -r "$t/stat" ] || continue
        printf '    %s %s wchan=%s\n' \
            "$(cut -d' ' -f3 "$t/stat" 2>/dev/null)" \
            "$(tr -d '\n' <"$t/comm" 2>/dev/null)" \
            "$(cat "$t/wchan" 2>/dev/null)"
    done | sort | uniq -c
}
prev_kwin=0
while true; do
    if [ -n "$follow" ] && [ -z "$gone_at" ] && ! kill -0 "$follow" 2>/dev/null; then
        gone_at=$(date +%s)
        echo "# $(date -Is) libri $follow exited; watching KWin for $after s more" >>"$log"
    fi
    if [ -n "$gone_at" ] && [ $(($(date +%s) - gone_at)) -ge "$after" ]; then
        echo "# $(date -Is) stop" >>"$log"
        exit 0
    fi
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
        for pid in ${follow:-$(pgrep -f 'target/(debug|release)/libri')}; do
            [ -d /proc/"$pid" ] || continue
            echo "  libri $pid threads:"
            threads "$pid"
        done
    } >>"$log"
    sleep 2
done
