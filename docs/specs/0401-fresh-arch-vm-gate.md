# N0401 — Fresh-Arch VM install gate

Status: phase A (feasibility) done 2026-11-11; phase B (the harness) not started.

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

Acceptance: all three tiers pass on dev-1 with packages built from one main commit, in
under 15 minutes per tier. The harness is at most about 300 lines. The Clasp greeter VM
(C0003) may reuse the boot part.

Not covered: performance, multiple monitors, the greeter and physical GPUs.
