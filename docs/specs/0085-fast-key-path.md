# N0085: instant volume and brightness key path

## Goal

Make volume, display brightness, and keyboard-light key presses reach the Nacre
daemon within 5 ms on the desktop test host. The current binding timestamps the
keypress before launching `nacrectl`; the owner measured 67–82 ms for
`nacrectl status` and 46–86 ms key-to-daemon on the laptop. The binding also
starts a second `nacrectl status` process before falling back to `wpctl` or
`brightnessctl`.

## Scope

- Profile the existing command startup remotely with `strace -c` (and `perf`
  when available), then use the smallest secure fast client/path that meets the
  measured gate.
- Update the compositor contract that generates Nacre's niri bindings. The
  daemon-down fallback must use a cheap connection failure result and must not
  spawn another `nacrectl`.
- Preserve daemon peer identity/socket validation and the current action
  semantics, including `key_ms` for OSD latency instrumentation.
- Bead twin: B0149 in the assigned Bead worktree, including held-repeat OSD
  smoothness and parity evidence. Cross-link its evidence/report here.

## Non-goals

No change to desktop hardware/lighting behavior, owner laptop runtime,
keybindings outside generated Nacre niri bindings, or unrelated CLI startup.
No install/deploy action is authorized by this task.

## Acceptance and evidence

- Remote profile captures the existing startup path; report the profile's
  command and dominant costs.
- Measure 100 calls on a remote host and report p50/p95 wall time before and
  after. The key client must reach the daemon in at most 5 ms from key event.
- Verify the daemon-down path reaches `wpctl`/`brightnessctl` without a second
  daemon CLI invocation; daemon-present errors must not trigger hardware
  fallback.
- On the desktop test machine, repeat-hold capture for two seconds: zero OSD
  dropped frames, and the displayed value changes within one frame of daemon
  publication. Record source/window/scale and exact commands in the evidence.
- Full Nacre CI: the complete `desktop` job from `.github/workflows/check.yml`
  in `ghcr.io/archlinux/archlinux:latest` on the selected remote CI host.
- Record removed product lines/files and report any performance/memory trade.

## Design choice

The key-specific `nacrectl key-step` branch runs before Tokio initialization.
It permits only the three hardware-step methods, validates the same private
socket endpoint and peer UID, then pipelines the normal protocol hello and
call in one write. It does not deserialize the large welcome/method catalogue.
It reads the two response frames as bounded raw lines and checks the small call
reply. Exit status
69 means only that the endpoint is absent or refused the connection; generated
bindings use that status to select the existing hardware fallback. No parallel
compatibility client is retained.

## Evidence

Remote startup profile (`strace -c -f`, absent-daemon baseline): 399 syscalls,
0.408 ms total syscall time. Dominant counts were 105 `mmap`, 53 `openat`, 49
`read`, and 43 `close`, consistent with dynamic loading and process setup;
this profile deliberately did not include a daemon response.

Remote 100-call comparison used the generated binding shape (`bash -c`,
`date +%s%3N`, then CLI) against a private Unix-socket fixture that returned a
128-entry synthetic welcome table and a normal call reply. Baseline
`nacrectl call audio.step`: p50 3.130 ms, p95 4.004 ms. Candidate
`nacrectl key-step audio.step`: p50 2.938 ms, p95 3.491 ms. Maximum was
4.346 ms for the candidate. This isolates shell/client/protocol cost; it does
not simulate hardware or prove physical key-repeat timing.

Full `.github/workflows/check.yml` `desktop` job passed in the remote Arch
container: `cargo fmt --check`, `cargo clippy -j2 --all-targets -- -D warnings`,
`cargo build -j2 --locked --examples`, `cargo test -j2 --quiet`, protocol
consumer, `tools/check.py` (501 tests; AI 120, Brain 52, shell-tools 490),
translation audits, native gallery, and ignored daemon memory test. The exact
runner script and output are retained in the task-local `.lead/evidence/`.

Bead twin evidence is in B0149. The paired QML/Bead OSD comparison records a
visible track-relative gradient difference. The current Bead hold capture
meets publish-to-visible within one 60 Hz frame but has a disagreement between
the shell `FRAME_JANK` marker (0) and compositor presentation interval analysis
(16 active intervals over 25 ms in the candidate; 15 in baseline). The Bead
frame-drop gate remains unresolved; no zero-drop claim is made here.
