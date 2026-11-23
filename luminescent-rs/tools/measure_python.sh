#!/usr/bin/env bash
# Measures the Python Luminescent the way PERFORMANCE.md asks, so the Rust one has numbers to beat.
#
#   luminescent-rs/tools/measure_python.sh                 # from the repo, with .venv
#   luminescent-rs/tools/measure_python.sh ~/Luminescent/Luminescent    # a downloaded build instead
#
# Options (environment variables):
#   RUNS=5            start-up runs to time (the first one warms the disk cache and is not counted)
#   IDLE_SECS=60      how long to sit idle before reading CPU and memory
#   COPY_FROM=dir     a data folder to copy and start with (your real one: ~/.local/share/Luminescent), so the
#                     numbers include your notes, themes and dashboard. It is copied, never used in place.
#   WITH_HOTKEYS=1    register hotkeys like a normal start. Quit your running Luminescent first, or the two
#                     fight over the keys. Without it the copy runs with LUMINESCENT_NO_HOOK=1 next to yours.
#
# It prints one Markdown table row to paste into rust-rewrite-plan/BASELINE.md.
set -euo pipefail

repo="$(cd "$(dirname "$0")/../.." && pwd)"
if [[ $# -gt 0 ]]; then
    cmd=("$@")
else
    py="$repo/.venv/bin/python"
    [[ -x "$py" ]] || py="$(command -v python3)"
    cmd=("$py" "$repo/Launcher.py")
fi
runs="${RUNS:-5}"
idle="${IDLE_SECS:-60}"
tick="$(getconf CLK_TCK)"

work="$(mktemp -d -t lummeasure-XXXXXX)"
trap 'kill "$pid" 2>/dev/null || true; rm -rf "$work"' EXIT
pid=""
ready_ms=0

fresh_home() {
    rm -rf "$work/home"
    mkdir -p "$work/home"
    if [[ -n "${COPY_FROM:-}" ]]; then
        cp -r "$COPY_FROM"/. "$work/home/"
        rm -f "$work/home/data/launcher.log"* "$work/home/data/starting.json"
    fi
}

export LUMINESCENT_HOME="$work/home"
[[ -n "${WITH_HOTKEYS:-}" ]] || export LUMINESCENT_NO_HOOK=1
if [[ -z "${WAYLAND_DISPLAY:-}${DISPLAY:-}" ]]; then
    export QT_QPA_PLATFORM=offscreen
fi

now_ms() { date +%s%3N; }

# start -> the "ready:" line in the log (written once every module has started). Sets $pid and $ready_ms;
# not run in $(...), which would keep $pid in a subshell where stop() can not see it.
start_once() {
    fresh_home
    local t0 log="$work/home/data/launcher.log"
    t0=$(now_ms)
    "${cmd[@]}" >"$work/stdout.txt" 2>&1 &
    pid=$!
    while ! grep -q " ready: " "$log" 2>/dev/null; do
        if ! kill -0 "$pid" 2>/dev/null; then
            echo "Luminescent exited before it was ready:" >&2
            tail -20 "$work/stdout.txt" >&2
            exit 1
        fi
        sleep 0.01
    done
    ready_ms=$(( $(now_ms) - t0 ))
}

stop() {
    kill "$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
    pid=""
}

cpu_ticks() { awk '{print $14 + $15}' "/proc/$1/stat"; }

# context switches of every thread: a fair count of how often the process wakes up
switches() {
    local total=0 n
    for f in /proc/"$1"/task/*/status; do
        n=$(awk '/^(voluntary|nonvoluntary)_ctxt_switches/ {s += $2} END {print s + 0}' "$f" 2>/dev/null || echo 0)
        total=$(( total + n ))
    done
    echo "$total"
}

echo "Timing $runs starts of: ${cmd[*]}" >&2
times=()
for i in $(seq 0 "$runs"); do
    start_once
    ms=$ready_ms
    stop
    if [[ $i -gt 0 ]]; then
        times+=("$ms")
        echo "  start $i: $ms ms" >&2
    else
        echo "  warm-up start: $ms ms (not counted)" >&2
    fi
done
sorted=$(printf '%s\n' "${times[@]}" | sort -n)
median=$(echo "$sorted" | awk '{a[NR]=$1} END {print (NR % 2) ? a[(NR+1)/2] : int((a[NR/2] + a[NR/2+1]) / 2)}')
best=$(echo "$sorted" | head -1)

echo "Sitting idle for $idle s..." >&2
start_once
sleep 3                                        # let the start-up work finish before counting
c0=$(cpu_ticks "$pid"); s0=$(switches "$pid"); w0=$(now_ms)
sleep "$idle"
c1=$(cpu_ticks "$pid"); s1=$(switches "$pid"); w1=$(now_ms)
rss_kb=$(awk '/^VmRSS/ {print $2}' "/proc/$pid/status")
threads=$(awk '/^Threads/ {print $2}' "/proc/$pid/status")
children=$(pgrep -P "$pid" | wc -l || true)
stop

secs=$(awk -v a="$w0" -v b="$w1" 'BEGIN {print (b - a) / 1000}')
cpu=$(awk -v c="$(( c1 - c0 ))" -v t="$tick" -v s="$secs" 'BEGIN {printf "%.2f", c / t / s * 100}')
wakeups=$(awk -v n="$(( s1 - s0 ))" -v s="$secs" 'BEGIN {printf "%.1f", n / s}')
rss_mb=$(awk -v k="$rss_kb" 'BEGIN {printf "%.0f", k / 1024}')

echo "Smoke test (builds every page and pop-up once, then quits)..." >&2
fresh_home
s0=$(now_ms)
if "${cmd[@]}" --smoke >"$work/smoke.txt" 2>&1; then smoke_ok=ok; else smoke_ok=FAILED; fi
smoke_ms=$(( $(now_ms) - s0 ))

desktop="${XDG_CURRENT_DESKTOP:-headless}"
[[ -n "${QT_QPA_PLATFORM:-}" && "$QT_QPA_PLATFORM" == offscreen ]] && desktop="headless"
cpu_model=$(awk -F': ' '/model name/ {print $2; exit}' /proc/cpuinfo)
hotkeys=$([[ -n "${WITH_HOTKEYS:-}" ]] && echo yes || echo no)
data=$([[ -n "${COPY_FROM:-}" ]] && echo "copy of mine" || echo fresh)

echo
echo "| Machine | Desktop | Data | Hotkeys | Start to ready (median / best of $runs) | Idle CPU ($idle s) | Wake-ups/s | Idle RSS | Threads | Child processes | Smoke test |"
echo "|---|---|---|---|---|---|---|---|---|---|---|"
echo "| $cpu_model, $(nproc) threads | $desktop | $data | $hotkeys | $median / $best ms | $cpu % | $wakeups | $rss_mb MB | $threads | $children | $smoke_ms ms ($smoke_ok) |"
