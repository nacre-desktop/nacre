#!/usr/bin/env bash
# One-command wrapper. Each step runs detached (tmux -L platform-TASK, or setsid where there is
# no tmux) so an SSH drop cannot kill it; ROOT/NAME.exit appears when it ends.
#   host.sh stage NACRE_COMMIT              build host: Noctalia + Nacre build container -> ROOT/stage
#   host.sh run PROFILE SEQUENCE [NAME]     build host: software-rendering container -> ROOT/out/NAME
#     BENCH_PERF=1 also perf-records shell and daemon over the interaction (diagnostic arms)
# Containers use the prebuilt nacre-ci/arch-nacre image (N0415); stage installs only Noctalia's
# packages, run only the session packages.
#   host.sh desktop PROFILE SEQUENCE [NAME] desktop TEST host: AMD node, shared lock -> ROOT/out/NAME
# ROOT defaults to ~/nacre-lead/n0308 and names the containers; this script lives in ROOT/harness.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=${ROOT:-$HOME/nacre-lead/n0308}
task=$(basename "$root")
step=${1:?stage|run|desktop}
mkdir -p "$root"
chmod go-w "$root"  # the Nacre daemon refuses state under a group- or world-writable directory
image=nacre-ci/arch-nacre:current
docker_run=("$HOME/nacre-lead/platform-slot-run.sh" docker run --rm --init --pids-limit 2048
  --cpus=6 --memory=10g --volume "$root:/task" --volume "$here:/harness:ro"
  --env "OWNER=$(id -u):$(id -g)")
if [[ "$step" != desktop ]]; then
  available=$(df -B1 --output=avail / | tail -1)
  ((available >= 150 * 1024 * 1024 * 1024)) || { echo "under 150 GiB free on /" >&2; exit 1; }
fi
case "$step" in
  stage)
    commit=${2:?full Nacre commit}
    [[ "$commit" =~ ^[0-9a-f]{40}$ ]] || { echo "need a full commit sha" >&2; exit 2; }
    name=$task-stage
    src=$root/src
    [[ -d $src/.git ]] || git clone -q https://github.com/nacre-desktop/nacre.git "$src"
    git -C "$src" fetch -q --no-recurse-submodules origin
    git -C "$src" checkout -q --detach "$commit"
    # deps/orient is private: on a host without GitHub access, unpack it there first, e.g.
    # git -C ~/Projects/orient archive REV | ssh HOST tar -x -C nacre-lead/n0308/src/deps/orient
    git -C "$src" submodule update -q --init --recursive 2> /dev/null \
      || [[ -f "$src/deps/orient/Cargo.toml" ]] || { echo "deps/orient missing" >&2; exit 1; }
    # shortcut: crates come from the network; N0415's read-only vendor set lacks Bead's protocols/
    # (bead-wayland includes ../../protocols/*.xml); mount it at /.cargo once ci-image vendors them.
    command=("${docker_run[@]}" --name "$name" --volume "$src:/workspace:ro" "$image"
      bash /harness/stage.sh /task/stage "$commit" "$(id -u):$(id -g)")
    ;;
  run)
    profile=${2:?short|full}
    sequence=${3:?e.g. nacre,noctalia,noctalia,nacre}
    out=${4:-$(date -u +%Y%m%dT%H%MZ)}
    name=$task-run-$out
    # Not in the image (reported to the Release lead): the session's labwc and PipeWire daemon,
    # Noctalia's runtime libraries and its notification sounds.
    inner='pacman -Syu --noconfirm --needed labwc pipewire pipewire-audio wireplumber \
        sound-theme-freedesktop libsecret libsodium sdbus-cpp libwebp libjxl libsndfile \
        libqalculate libxml2 md4c tomlplusplus libical jemalloc polkit ${BENCH_PERF:+perf} \
      && { [[ -z ${BENCH_PERF:-} ]] || setcap cap_perfmon+ep /usr/bin/perf; } \
      && groupadd -g "${OWNER#*:}" bench && useradd -u "${OWNER%:*}" -g bench -M bench \
      && install -d -o bench -g bench /task/out /tmp/bench \
      && exec setpriv --reuid=bench --regid=bench --init-groups \
        env HOME=/tmp/bench BENCH_LABEL="$4" LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe \
        bash /harness/run.sh /task/stage "/task/out/$1" "$2" "$3" ${BENCH_PERF:+--perf}'
    perf=()
    [[ -z ${BENCH_PERF:-} ]] || perf=(--cap-add PERFMON --env BENCH_PERF=1)
    command=("${docker_run[@]}" "${perf[@]}" --name "$name" "$image"
      bash -c "$inner" bash "$out" "$profile" "$sequence" "$(hostname -s) $(date -u +%F)")
    ;;
  desktop)
    profile=${2:?short|full}
    sequence=${3:?e.g. nacre,noctalia,noctalia,nacre,nacre,noctalia}
    out=${4:-$(date -u +%Y%m%dT%H%MZ)}
    name=$task-desktop-$out
    node=
    for dev in /sys/class/drm/renderD*; do
      if [[ $(cat "$dev/device/vendor") == 0x1002 ]]; then node=/dev/dri/${dev##*/}; fi
    done
    [[ -n "$node" ]] || { echo "no AMD render node" >&2; exit 1; }
    command=(env "XDG_RUNTIME_DIR=/run/user/$(id -u)" "BENCH_LABEL=desktop $(date -u +%F)"
      bash "$here/run.sh" "$root/stage" "$root/out/$out" "$profile" "$sequence"
      --render-node "$node" --lock "$HOME/nacre-lead/desktop-heavy.lock" --mem-rail-gib 3
      --niri /usr/bin/niri)
    ;;
  *) echo "usage: host.sh stage COMMIT | host.sh run|desktop PROFILE SEQUENCE [NAME]" >&2; exit 2 ;;
esac
script=$root/$name.sh
[[ -e "$script" && ! -e "$root/$name.exit" ]] && { echo "already running: $name" >&2; exit 1; }
{
  echo '#!/usr/bin/env bash'
  printf 'PLATFORM_LOCK_WAIT=14400 nice -n 10'
  printf ' %q' "${command[@]}"
  printf ' > %q 2>&1\n' "$root/$name.log"
  printf 'echo $? > %q\n' "$root/$name.exit"
} > "$script"
rm -f "$root/$name.exit"
if command -v tmux > /dev/null; then
  tmux -L "platform-$task" new-session -d -s "$name" "bash $script"
else
  setsid -f bash "$script" < /dev/null > /dev/null 2>&1
fi
echo "started $name; log: $root/$name.log"
echo "wait: timeout 540 bash -c 'until [ -e $root/$name.exit ]; do sleep 20; done'; cat $root/$name.exit"
