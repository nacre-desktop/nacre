#!/usr/bin/env bash
# Read-only Linux sampler. No sudo, settings, inhibitors, tracing or daemons.
set -euo pipefail
export LC_ALL=C
command -v gawk >/dev/null || { echo "GNU awk is required; no packages installed." >&2; exit 2; }
umask 077
root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
duration=600
interval=10
lead_in=0
output=""
revision=unknown
scope='quickshell|nacre-shell|orient|codex|claude|/nacre/(shell|brain|ai)/'
setup_only=false
protected=false
protected_data=""
summary_dir=""
live_source="${NACRE_QS_CONFIG:-$HOME/.config/quickshell/nacre}"
usage() {
    cat <<'HELP'
Usage: tools/measure-power.sh [--duration SECONDS] [--interval SECONDS] [--lead-in SECONDS]
       [--output NEW_DIRECTORY] [--revision LIVE_REVISION] [--scope ERE] [--live-source DIRECTORY] [--setup-only]
       [--protected | --summarize DIRECTORY] [--protected-data FILE]
Defaults: 600 seconds, 10-second samples, ignored .power-baseline/ timestamp directory.
--protected streams only root-readable RAPL/PSS as TSV to stdout.
--summarize regenerates a completed run summary; --protected-data adds that stream.
Requires Bash, GNU awk and standard Linux utilities.
--scope matches command lines privately in memory, then includes descendants.
Default scope includes QML/native shells, Orient, Nacre helpers and AI workers.
Raw files contain process names/IDs, not arguments, window titles or environments.
Root may read RAPL/PSS; this script never requests elevation or changes permissions.
Package C states and true wakeups are unavailable; context switches are a proxy.
Screen-on/app-free conditions must be arranged by the owner, without this script.
Do not work/build/browse during a baseline. Ctrl-C leaves partial raw data.
HELP
}
while (($#)); do
    case "$1" in
        --duration|--interval|--lead-in|--output|--revision|--scope|--live-source|--protected-data|--summarize)
            (($# >= 2)) || { usage >&2; exit 2; }
            case "$1" in
                --duration) duration=$2;; --lead-in) lead_in=$2;; --interval) interval=$2;;
                --output) output=$2;; --protected-data) protected_data=$2;; --summarize) summary_dir=$2;; --revision) revision=$2;; --scope) scope=$2;; --live-source) live_source=$2;;
            esac
            shift 2;;
        --protected) protected=true; shift;;
        --setup-only) setup_only=true; shift;;
        -h|--help) usage; exit;;
        *) usage >&2; exit 2;;
    esac
done
[[ $duration =~ ^[1-9][0-9]*$ && $interval =~ ^[1-9][0-9]*$ ]] || exit 2
[[ $lead_in =~ ^[0-9]+$ ]] || exit 2
((duration % interval == 0)) || { echo 'Duration must be a multiple of interval.' >&2; exit 2; }
value() { local v; if [[ -r $1 ]] && IFS= read -r v < "$1"; then printf '%s' "$v"; else printf 'NA'; fi; }
clock() { read -r monotonic _ < /proc/uptime; }
if $protected; then
    # stdout redirection is performed by the owner shell, not by root.
    protected_pss() {
        gawk -v now="$monotonic" -v scope="$scope" -v observer="$$" '
            BEGINFILE {if (ERRNO != "") nextfile}
            function group(pid,depth, inherited) {
                if(pid==observer) return -1
                if(depth>100 || !(pid in parent)) return 0
                inherited=parent[pid] ? group(parent[pid],depth+1) : 0
                if(inherited==-1) return -1
                return seed[pid] || inherited
            }
            {
                pid=FILENAME; sub("/proc/","",pid); sub("/stat","",pid)
                line=$0; sub(/^.*\) /,"",line); split(line,f," ")
                parent[pid]=f[2]; start[pid]=f[20]
                cmd=""; path="/proc/" pid "/cmdline"; getline cmd < path; close(path)
                gsub(/\x00/," ",cmd); seed[pid]=(cmd ~ scope)
            }
            END {
                for(pid in parent) if(group(pid,0)==1) {
                    path="/proc/" pid "/smaps_rollup"; mem="NA"
                    while((getline line < path)>0) if(line ~ /^Pss:/) {split(line,f," "); mem=f[2]}
                    close(path)
                    print "pss\t" now "\t" pid ":" start[pid] "\t" mem "\tNA"
                }
            }' /proc/[0-9]*/stat
    }
    sleep "$lead_in"
    clock; protected_begin=$monotonic
    for ((sample=0; sample<=duration/interval; sample++)); do
        if ((sample>0)); then
            clock
            delay=$(awk -v start="$protected_begin" -v n="$sample" -v step="$interval" -v now="$monotonic" 'BEGIN {d=start+n*step-now; printf "%.3f",(d>0?d:0)}')
            sleep "$delay"
        fi
        clock
        for dir in /sys/class/powercap/intel-rapl:[0-9]*; do
            [[ $(value "$dir/name") == package-* ]] || continue
            printf 'rapl\t%s\t%s\t%s\t%s\n' "$monotonic" "$dir" "$(value "$dir/energy_uj")" "$(value "$dir/max_energy_range_uj")"
        done
        if ((sample==0 || sample==duration/interval)); then protected_pss; fi
    done
    exit
fi
if [[ -n $summary_dir ]]; then
    output=$summary_dir
    [[ -f $output/processes.tsv && -f $output/threads.tsv && -f $output/counters.tsv ]] || exit 2
else
    [[ -n $output ]] || output="$root/.power-baseline/$(date -u +%Y%m%dT%H%M%SZ)-$$"
    [[ ! -e $output ]] || { echo 'Output must be a new directory.' >&2; exit 2; }
    mkdir -p -- "$output"
fi
trap 'echo "Interrupted; partial raw data: $output" >&2; exit 130' INT TERM
if [[ -f $live_source ]]; then live_source=$(dirname -- "$live_source"); fi
fingerprint() {
    if [[ -d $live_source ]]; then
        (cd -- "$live_source" && find . -type f \( -name '*.qml' -o -name '*.js' -o -name qmldir \) -print0 | sort -z | xargs -0 -r sha256sum | sha256sum)
    else echo 'unavailable'; fi
}
if [[ -z $summary_dir ]]; then
{
    printf 'utc=%s\nduration_s=%s\ninterval_s=%s\nlive_revision=%s\n' "$(date -u +%FT%TZ)" "$duration" "$interval" "$revision"
    printf 'sampler_revision=%s\nkernel=%s\n' "$(git -C "$root" rev-parse HEAD 2>/dev/null || echo unknown)" "$(uname -r)"
    awk -F ': ' '/model name/ {print "cpu=" $2; exit}' /proc/cpuinfo
    command -v lspci >/dev/null && lspci -nn | awk '/VGA|Display|3D/ {print "gpu=" $0}'
    for f in /sys/devices/system/cpu/cpufreq/policy*/{scaling_driver,scaling_governor,energy_performance_preference} \
        /sys/firmware/acpi/platform_profile /sys/class/backlight/*/{brightness,actual_brightness,max_brightness} \
        /sys/class/power_supply/*/{online,status}; do
        [[ -e $f ]] && printf '%s=%s\n' "$f" "$(value "$f")"
    done
    if command -v busctl >/dev/null; then
        busctl get-property net.hadess.PowerProfiles /net/hadess/PowerProfiles net.hadess.PowerProfiles ActiveProfile || true
    fi
    bash "$root/nacre/shell-tools/compositor_backends/power-baseline.sh" || true
    printf 'live_source_fingerprint_sha256=%s\n' "$(fingerprint)"
    marker="$HOME/.local/state/nacre/releases/current.json"
    if [[ -r $marker ]]; then
        sed -nE 's/^  "(revision|baseRevision)": "([a-f0-9]+)".*/deployment_\1=\2/p' "$marker"
    fi
    echo 'package_cstates=unavailable; no installed MSR reader used'
    echo 'true_per_process_wakeups=unavailable; voluntary context switches are only a proxy'
    printf 'scope_ere=%s\n' "$scope"
} > "$output/setup.txt" 2> "$output/setup-errors.txt"
shopt -s nullglob
rapl=()
for dir in /sys/class/powercap/intel-rapl:[0-9]*; do
    [[ $(value "$dir/name") == package-* ]] && rapl+=("$dir")
done
rc6=(/sys/class/drm/card[0-9]*/gt/gt[0-9]*/rc6_residency_ms)
if ((${#rc6[@]} == 0)); then rc6=(/sys/class/drm/card[0-9]*/power/rc6_residency_ms); fi
printf 'kind\ttime_s\tidentity\tvalue\trange\n' > "$output/counters.tsv"
printf 'time_s\tpid\tstart_ticks\tgroup\tcomm\tcpu_ticks\tpss_kib\n' > "$output/processes.tsv"
printf 'time_s\tpid\tstart_ticks\ttid\tthread_start\tvoluntary\tinvoluntary\n' > "$output/threads.tsv"
fi
# A single awk per scan avoids launching commands for every PID/thread.
snapshot() {
    clock
    local f dir
    for dir in "${rapl[@]}"; do
        printf 'rapl\t%s\t%s\t%s\t%s\n' "$monotonic" "$dir" "$(value "$dir/energy_uj")" "$(value "$dir/max_energy_range_uj")"
    done >> "$output/counters.tsv"
    for f in "${rc6[@]}"; do
        printf 'rc6\t%s\t%s\t%s\tNA\n' "$monotonic" "$f" "$(value "$f")"
    done >> "$output/counters.tsv"
    for f in /sys/class/drm/card[0-9]*-*/dpms; do
        [[ $(value "$(dirname -- "$f")/status") == connected ]] || continue
        printf 'display\t%s\t%s\t%s\tNA\n' "$monotonic" "$f" "$(value "$f")"
    done >> "$output/counters.tsv"
    for f in /sys/class/power_supply/*/{online,status,energy_now,power_now}; do
        printf 'supply\t%s\t%s\t%s\tNA\n' "$monotonic" "$f" "$(value "$f")"
    done >> "$output/counters.tsv"
    gawk -v now="$monotonic" -v scope="$scope" -v observer="$$" -v pss="$1" \
        -v processes="$output/processes.tsv" -v threads="$output/threads.tsv" '
        function readfile(path, result) {
            result=""; getline result < path; close(path); return result
        }
        function group(pid, depth) {
            if (pid == observer) return "observer"
            if (depth > 100 || !(pid in parent)) return "other"
            if (parent[pid] && group(parent[pid], depth+1)=="observer") return "observer"
            if (seed[pid]) return "nacre"
            return parent[pid] ? group(parent[pid], depth+1) : "other"
        }
        BEGINFILE {if (ERRNO != "") nextfile}
        {
            pid=FILENAME; sub("/proc/", "", pid); sub("/stat", "", pid)
            line=$0; name=line; sub(/^[0-9]+ \(/,"",name); sub(/\) .*$/,"",name)
            sub(/^.*\) /,"",line); split(line, f, " ")
            if (length(f)<20) next
            parent[pid]=f[2]; start[pid]=f[20]; cpu[pid]=f[12]+f[13]
            gsub(/[\t\r\n]/,"?",name); comm[pid]=name
            cmd=readfile("/proc/" pid "/cmdline"); gsub(/\x00/," ",cmd)
            seed[pid]=(cmd ~ scope)
        }
        END {
            for (pid in parent) {
                g=group(pid,0); mem="NA"
                if (pss && g=="nacre") {
                    path="/proc/" pid "/smaps_rollup"
                    while ((getline line < path)>0) if (line ~ /^Pss:/) {split(line,f," "); mem=f[2]}
                    close(path)
                }
                print now "\t" pid "\t" start[pid] "\t" g "\t" comm[pid] "\t" cpu[pid] "\t" mem >> processes
            }
        }' /proc/[0-9]*/stat 2>> "$output/read-errors.txt"
    gawk -v now="$monotonic" '
        BEGINFILE {if (ERRNO != "") nextfile}
        FNR==1 {
            if (tid!="") emit()
            path=FILENAME; split(path,p,"/"); pid=p[3]; tid=p[5]; v="NA"; iv="NA"
        }
        /^voluntary_ctxt_switches:/ {v=$2}
        /^nonvoluntary_ctxt_switches:/ {iv=$2}
        function start(path, line,f) {
            line=""; getline line < path; close(path); sub(/^.*\) /,"",line); split(line,f," "); return f[20]
        }
        function emit( s,ts) {
            s=start("/proc/" pid "/stat"); ts=start("/proc/" pid "/task/" tid "/stat")
            if (s!="" && ts!="") print now "\t" pid "\t" s "\t" tid "\t" ts "\t" v "\t" iv
        }
        END {if (tid!="") emit()}' /proc/[0-9]*/task/[0-9]*/status >> "$output/threads.tsv" 2>> "$output/read-errors.txt"
}
summarize() {
    awk -F '\t' -v perprocess="$output/process-summary.tsv" -v hz="$(getconf CLK_TCK)" -v procs="$output/processes.tsv" '
        FNR==1 {next}
        FILENAME==procs {
            k=$2 ":" $3; grp[k]=$4; name[k]=$5; pid[k]=$2
            if (k in prev) {d=$6-prev[k]; if(d>=0) {cpu[k]+=d/hz; totalcpu[$4]+=d/hz}}
            prev[k]=$6
            if (!(k in first)) first[k]=$1
            last[k]=$1
            if ($4=="nacre") {expected[$1]++; if ($7!="NA") {pss[$1]+=$7; count[$1]++}}
            if(!begin || $1<begin) begin=$1; if($1>end) end=$1
            next
        }
        {
            k=$2 ":" $3; tk=k ":" $4 ":" $5
            if ($6!="NA" && tk in vprev) {
                d=$6-vprev[tk]; if(d>=0) {vol[k]+=d; totalvol[grp[k]]+=d}
            }
            if($6!="NA") vprev[tk]=$6
        }
        END {
            elapsed=end-begin
            printf "Elapsed: %.2f s\nCPU seconds and voluntary-switch proxy/s:\n",elapsed
            for(g in totalcpu) printf "  %s: %.3f CPU s, %.2f switches/s\n",g,totalcpu[g],(elapsed>0?totalvol[g]/elapsed:0)
            if(count[begin]) printf "Nacre start PSS: %.2f MiB (%d/%d readable processes)\n",pss[begin]/1024,count[begin],expected[begin]
            else print "Nacre start PSS: unavailable"
            if(count[end]) printf "Nacre end PSS: %.2f MiB (%d/%d readable processes)\n",pss[end]/1024,count[end],expected[end]
            else print "Nacre end PSS: unavailable"
            print "Per-process observed deltas (short-lived processes may be missed):"
            for(k in grp) printf "%.2f\t%.3f\t%s\t%s\t%s\n",(elapsed>0?vol[k]/elapsed:0),cpu[k],grp[k],pid[k],name[k] > perprocess
        }' "$output/processes.tsv" "$output/threads.tsv"
    {
        if [[ -n $protected_data ]]; then
            awk -F '\t' 'NR==1 || $1!="rapl"' "$output/counters.tsv"
            cat -- "$protected_data"
        else cat "$output/counters.tsv"; fi
    } | awk -F '\t' '
        NR==1 {next}
        $1=="rapl" || $1=="rc6" {
            k=$3; if($4=="NA") {missing[k]=1; next}
            if(k in prev) {
                dt=$2-t[k]; d=$4-prev[k]
                if(d<0 && $1=="rapl" && $5!="NA") d+=$5
                if(dt>0 && d>=0) {
                    n[k]++; elapsed[k]+=dt; delta[k]+=d
                    x=($1=="rapl" ? d/1e6/dt : d/1000/dt*100)
                    values[k,n[k]]=x
                }
            }
            type[k]=$1; prev[k]=$4; t[k]=$2
        }
        END {
            for(k in prev) {
                if(!n[k]) continue
                # 60 intervals: insertion sort and nearest-rank p95.
                for(i=2;i<=n[k];i++) {x=values[k,i];j=i-1;while(j>=1 && values[k,j]>x){values[k,j+1]=values[k,j];j--} values[k,j+1]=x}
                rank=int(.95*n[k]); if(rank<.95*n[k]) rank++
                mean=(type[k]=="rapl" ? delta[k]/1e6/elapsed[k] : delta[k]/1000/elapsed[k]*100)
                printf "%s %s: average %.3f %s, p95 %.3f, %d intervals\n",type[k],k,mean,type[k]=="rapl"?"W":"%",values[k,rank],n[k]
            }
            for(k in missing) printf "Unavailable counter: %s\n",k
        }'
    if [[ -n $protected_data ]]; then
        awk -F '\t' '$1=="pss" {expected[$2]++; if($4!="NA") {sum[$2]+=$4; n[$2]++}} END {for(t in expected) printf "Protected Nacre PSS at uptime %s: %.2f MiB (%d/%d readable processes)\n",t,sum[t]/1024,n[t],expected[t]}' "$protected_data"
    fi
    echo 'Top voluntary context-switch proxies/s (not actual wakeup sources):'
    sort -t $'\t' -k1,1nr "$output/process-summary.tsv" | sed -n '1,15p'
    if [[ -f $output/end-conditions.txt ]]; then cat "$output/end-conditions.txt"; fi
    awk -F '\t' '$1=="display" {n[$3]++; if($4!="On") bad[$3]++} END {for(k in n) printf "Display %s: %d/%d samples On\n",k,n[k]-bad[k],n[k]}' "$output/counters.tsv"
    echo 'True per-process wakeups and package C-state residency: unavailable.'
    printf 'Raw data: %s\n' "$output"
}
if [[ -n $summary_dir ]]; then summarize > "$output/summary.txt"; cat "$output/summary.txt"; exit; fi
if $setup_only; then snapshot 1; echo "Preflight only: $output"; exit; fi
sleep "$lead_in"
printf 'Sampling begins now: %s; duration %s seconds.\n' "$(date -u +%FT%TZ)" "$duration"
snapshot 1
clock
begin=$monotonic
for ((sample=1; sample<=duration/interval; sample++)); do
    clock
    delay=$(awk -v start="$begin" -v n="$sample" -v step="$interval" -v now="$monotonic" 'BEGIN {d=start+n*step-now; printf "%.3f",(d>0?d:0)}')
    sleep "$delay"
    if ((sample==duration/interval)); then snapshot 1; else snapshot 0; fi
done
{
    printf 'live_source_fingerprint_end_sha256=%s\n' "$(fingerprint)"
    for f in /sys/class/backlight/*/{brightness,actual_brightness} /sys/class/power_supply/*/{online,status}; do
        [[ -e $f ]] && printf '%s=%s\n' "$f" "$(value "$f")"
    done
} > "$output/end-conditions.txt"
summarize > "$output/summary.txt"
cat "$output/summary.txt"
printf 'Finished: %s. You can return.\n' "$(date -u +%FT%TZ)"
