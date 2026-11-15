# N0401 — Fresh-Arch VM install gate

Status: fresh installs pass every stage on all three tiers at main 86433e4a (N0418, below);
the upgrade stage waits on N0428. Rerun both at the cut.

v0.1 ships only when a fresh Arch Linux VM can install Nacre from a signed repository,
start a working session on each tier, and remove it again. This unit builds that check.

## Phase A findings (dev-1, QEMU 9.2 with KVM)

- The official Arch cloud image (verified by SHA-256) is booted through a qcow2 overlay, a
  cloud-init seed (a `tester` user, its own SSH key) and user networking with a forwarded
  SSH port. SSH is ready about 90 s after a cold boot and 48 s after a reboot. Two gotchas:
  sshd starts only after pacman-init and time sync, so wait up to five minutes; and never
  run `systemctl enable --now` inside cloud-init, because it deadlocks the boot.
- Rendering: niri's tty backend refuses software EGL, so llvmpipe gives no outputs.
  virgl works for both compositors: `-device virtio-vga-gl -display
  egl-headless,rendernode=/dev/dri/renderD128`, with `-device virtio-rng-pci`. In the
  guest, EGL reports virgl, `niri msg outputs` and `hyprctl monitors` list an output,
  and `grim` captures.
- Sessions start without a person through a tty1 autologin drop-in for getty and a
  `.bash_profile` that runs the selected session.
- The published October snapshot (r406) was installed with `install-arch.sh`, one `y`
  for each package transaction, then `nacre setup --non-interactive --profile default
  --compositor niri --layout us --yes --install-deps`. `nacre session niri` started
  the shell, and the screenshot showed it. The bootstrap's final bare `nacre setup`
  exits 2 when there is no terminal, which is expected.
- That snapshot has no `nacre uninstall`. After `pacman -Rns`, its user files, units,
  links and repository trust stayed behind, so it would fail the removal check.
  Current main has `nacre uninstall --plan/--purge`.
- Timings: package install 34 s, setup 7 s, package removal 18 s.

## Phase B: the harness

One script, `packaging/arch/test-vm.py`, next to the other package proofs. It needs no
service and no framework.

Input: `--tier nacre-niri|niri|hyprland`, then either `--packages DIR` (archives from
the package workflow or a local build, served to the VM as a temporary repository signed
with a throwaway key, the way `test-delivery.py` does) or `--repo URL`, and also
`--base IMAGE` and `--out DIR`.

For each run:
1. A fresh overlay, seed, key and free port. The VM runs under `flock` on a caller-given
   lock and is stopped by its exact PID in a cleanup trap.
2. Record a file inventory of `/etc` and the user's HOME, plus `pacman -Qq`.
3. Run `install-arch.sh --plan`, then the real bootstrap, then the non-interactive setup
   for the tier.
4. Start the session from tty1. Pass only if: the compositor has a non-empty output,
   `nacre-shell.service` is active, the journal has no `UI_ERROR`, and the screenshot
   is not blank.
5. `nacre update` with nothing pending ends cleanly (after N0403).
6. `nacre uninstall --plan`, `nacre uninstall --purge`, then `pacman -Rns` of the Nacre
   packages. Fail on any surviving Nacre file in HOME, broken links into `/usr/share/nacre`
   or `/usr/lib/nacre`, or user units. The repository config and key may stay, since
   the docs tell users to remove them, and so may packages setup installed on purpose.
7. Print a table of each stage with pass or fail and its time. Write logs and the
   screenshot to `--out`. Raw logs stay out of Git.

Upgrade stage (N0418): with `--upgrade-from OLD`, steps 1-4 install and start the session
from the older archive set. The harness then appends a comment to a file setup seeds once and
then leaves to the user (kept when edited, spec 0101), listed by `nacre status --files`:
`~/.config/nacre/niri/custom.kdl` on the Niri tiers, `~/.config/nacre/hypridle.local.conf`
on Hyprland. It rebuilds the guest repository from `--packages` (same name and key, new
database) and runs `nacre update` on a pty in the session environment, as a user in a
terminal would, taking every default and answering yes to the update and to applying setup.
It passes only on the outcome: `pacman -Q nacre` shows the new version, the summary has
`nacre OLD -> NEW`, setup kept the tier's compositor, the restarted session passes step 4
again, and the edit is unchanged. Steps 5-7 follow. Step 5 alone proves little: at main
86433e4a `nacre update` only hands off to the running shell and exits 0 (N0422, fix N0428).

Acceptance: all three tiers pass on dev-1 with packages built from one main commit, in
under 15 minutes per tier. The harness is at most about 300 lines. The Clasp greeter VM
(C0003) may reuse the boot part.

Not covered: performance, multiple monitors, the greeter and physical GPUs.

## Phase B result (2026-11-12)

Core and all session splits were built from main `0716a15020b0`, with native
Orient built from the locked `3efe8d1e7fb5` source and the reviewed `1028c5b919ab`
fork archive. Fresh verified Arch cloud overlays ran on KVM/virgl, 4 vCPU/6 GiB.
The harness uses the publisher's bootstrap rendering, a throwaway signed guest
repository, and baseline inventories that include its login/network fixtures.

| Tier | Bootstrap/setup | TTY1, outputs, shell, journal, image | Purge inventory | Time |
|---|---|---|---|---|
| nacre-niri | PASS | PASS | FAIL: Bead state remains | 7m36s |
| niri | PASS | PASS | FAIL: Bead state remains | 9m44s |
| hyprland | PASS | PASS | FAIL: Bead state remains | 8m49s |

Uninstall plan/purge and package removal exit successfully, but Nacre creates
unrecorded `~/.local/state/bead/ui-health-*` and `bead/ui/*` files. They survive
purge: `crates/nacre-shell/src/state.rs:38-46` selects the generic Bead state
directory, `crates/nacre-shell/src/main.rs:560-570` writes recovery state beside it, while
`tools/nacre_modules.py:292-297` purges only Nacre-owned directories. The gate
rejects this residue; it is not allowlisted. The safe N0402 fix must scope all
Nacre UI/recovery persistence under Nacre-owned state, preserving other Bead
apps and launched apps' XDG environment. This exceeds the allowed small
setup/uninstall-only fix; no product patch was applied.

The cut predates N0403, so update is SKIP. Main advanced during the build/proof;
the next unblocked cut must include its no-op update check. Screenshots and raw
stage logs remain outside Git. The full hosted CI gate remains with integration;
this unit passed Ruff and 12 focused driver/repository tests in a fresh Arch
container. Mirror timeouts required an isolated Niri retry; the VM fixture uses
alternate official mirrors and serial package downloads, with signature checks
unchanged.

## Rerun 2026-11-14 (N0417)

Packages built from main `b94eba50` (core r3282+, nacre-keyring, Orient d2cf2da, nacre-niri r3079),
fresh overlays on dev-1 with KVM/virgl. The first rerun found two release bugs, both fixed before this run:
core lacked `libpipewire`, which the helper and shell link, so setup failed on a fresh system (85f66126);
and `--purge` kept theme outputs that the palette had rewritten, such as `~/.config/hypr/hyprlock.conf` (b94eba50).

| Tier | Every stage (prepare → leftovers) | update | keyring populated | Time |
|---|---|---|---|---|
| nacre-niri | PASS | PASS | PASS | 2m47s |
| niri | PASS | PASS | PASS | 2m04s |
| hyprland | PASS | PASS | PASS | 2m50s |

The packages and the stage log are kept on dev-1 under `~/nacre-lead/release-packages/b94eba50…` for the cut.


## Rerun 2026-11-14 evening (N0418)

Core and splits built from main `86433e4a` (r3337, lean profile, `makepkg --nocheck` after green
hosted Source checks), with Orient 0.1.0-dev.7 and nacre-niri r3079 reused from the `b94eba50` set.
Fresh overlays on dev-1 with KVM/virgl; the upgrade run starts from the `b94eba50` set.

| Tier | Mode | Stages | nacre-niri | Time | Main SHA |
|---|---|---|---|---|---|
| nacre-niri | fresh | all PASS (prepare → leftovers) | r3079 | 2m56s | 86433e4a |
| niri | fresh | all PASS | r3079 | 2m09s | 86433e4a |
| hyprland | fresh | all PASS | r3079 | 1m55s | 86433e4a |
| nacre-niri | upgrade from b94eba50 | prepare → session PASS; upgrade FAIL: `nacre update` exits 0 without updating, nacre stays r3292 | r3079 | 2m01s | 86433e4a |
| nacre-niri | upgrade, fixed CLI both sides (86433e4a+5abe472c pkgrel 1 → 2) | prepare → session PASS; upgrade unfinished: `nacre update` reached `sudo pacman -Syu` on a loaded host and was stopped after 6 min at the night's hard stop | r3110 | — | 86433e4a+5abe472c |

An upgrade runs the old package's `nacre update`, so the N0428 fix cannot reach installs that
predate it; they need one manual `sudo pacman -Syu`, then `nacre setup`. The other two tiers
run the same old command and were not repeated. The real upgrade proof needs the fix on both
sides: dev-1 keeps `86433e4a4+5abe472c` (pkgrel 1) and the same build repackaged as pkgrel 2 for
that run, plus nacre-niri r3110 (`f405f1a9`). With the fix, `nacre update` runs `updates.sh`
(seen in the guest), so the fresh update stage now drives it on a pty too. The fresh
nacre-niri rerun on that set did not run before the hard stop.
