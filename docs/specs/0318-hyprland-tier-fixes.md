# N0318 — Hyprland tier-2 fixes from the N0316 smoke

Status: B1 and B5 fixed in Nacre; B3 is a harness artifact (no Nacre change); B9's fix is in
Orient and waits for the lead (local Orient commit, not pushed). Bug numbers are N0316's
(`n0316-tier2-smoke/evidence.md`, Hyprland 0.56.2, aquamarine 0.15.1).

## B1: overview live previews (windowCapture)

Every Hyprland overview capture asset was rejected (`UI_ERROR … asset not in this
generation's snapshot`, 54 in one run, a 26 s panel stall) and the tiles stayed icons.
`compositor/hyprland.rs` now advertises `windowCapture=false` (snapshot test and the two CLI
reference fixtures updated), so Settings hides the live-previews row as N0301 intended.

With Shell's N0234 the overview leases captures only when the compositor topic advertises this
bit, so on Hyprland the tiles now keep their app icons and request no capture.

## B5: Settings, Colors and Widgets open at their minimum size

Hyprland maps these floating Bead windows at their xdg min size (Settings 544x480, Widgets
420x360). Two causes in the generated `hypr/conf` rules:

- The Widgets (and Thunar) rule had `size="70% 75%"`. Hyprland 0.56's Lua rules take two
  expressions (`monitor_w`, `monitor_h`, …) and reject it: `failed to parse 70% 75% as an
  expression`. The contract now says `["monitor_w*0.7","monitor_h*0.75"]`, the form Dolphin
  already used; the niri output is unchanged (`proportion 0.7` / `0.75`).
- Settings and Colors had no size. They get Hyprland-only sizes (`backend.hyprland.size`, newly
  allowed by the generator) that mirror what the app picks on niri: Settings 75% x 80% of the
  monitor clamped to 720x540 … 1100x800 (`settings/behavior.luau` window policy), Colors
  1100x760. Welcome needs none: its min size is its natural size (1100x800).

The desktop's muparser 2.3.5 (Hyprland's expression engine) rejects `70%` with exactly the
logged error and evaluates the new sizes as intended: Settings 1100x800 at 1920x1080, 1024x614
at 1366x768, 960x576 at 1280x720; Widgets 1344x810 at 1920x1080.

## B3: bar clicks in the nested Hyprland session (harness artifact)

The nested session has no pointer device of its own (headless labwc parent with
`WLR_LIBINPUT_NO_DEVICES=1`, so aquamarine creates none). The harness's `virtual-pointer`
creates a device, moves, clicks and exits, so every click adds and then removes the seat's
only pointer. Hyprland (0.56.2) follows devices: `wl_seat.capabilities` gains and loses
`pointer` per click (the log shows "setPointerFocus without a valid mouse set" in between).
Bead releases its `wl_pointer` on removal and asks for a new one on the next add, but that
request arrives after Hyprland already handled the tool's motion. Hyprland's new pointer
resource then gets `enter` at (-1, -1) (`CWLPointerResource` constructor, `protocols/core/Seat.cpp`)
and no motion follows before the press, so the click lands at (-1, -1), outside every bar item.
Hover-opened targets (the control-centre edge handle, `clickEdgeMenus=false`) still open, which
is why only those "clicks" passed. niri keeps the pointer capability for the whole session,
and a real Hyprland session keeps its mouse or touchpad, so neither path loses the coordinates.
No Nacre input-region, layer-rule (`ignore_alpha` only affects blur) or scale cause.

Evidence: in N0316's `hypr-smoke2` Hyprland log every bar click is preceded by "New virtual
mouse created" and followed by "Pointer at … removed". For all 12 bar clicks Bead set cursor
shape 1 (default) for enter, press and release, the shape it computes outside every item; only
the edge-handle click produced shape 4 (pointer). A real-input check needs a session with a
persistent pointer: the N0318 harness keeps one `virtual-pointer … hover` device alive for its
second click round.

## B9: video wallpaper selection from a terminal

`nacre-helper wallpaper-media select` publishes the palette in-process through the Orient
crates (`deps/orient`). Orient's `decoder::subprocess` starts ffprobe/ffmpeg in their own
process group (a background group on the caller's terminal) with the terminal inherited as
stdin. ffmpeg's keyboard handling then stops it (SIGTTIN/SIGTTOU) until the 20 s deadline:
"Animation decoder timed out". Nacre spawns no child on this path, so there is no Nacre call
site to fix. Fix (Orient, local commit `d2cf2da` on `claude/n0318-ffmpeg-stdin`, not pushed):
`Stdio::null()` for every decoder child plus `-nostdin` for ffmpeg, with a test that the
child's fd 0 is `/dev/null`. Landing it needs the lead's decision, then a `deps/orient` pin and
`dependencies.lock.json` bump in Nacre.

Repro (arch-nacre container, `orient analyze clip.mp4` run on a pty with `script`): Orient
main 79446d3 fails after 20 s with "Animation decoder timed out"; with the fix it returns the
analysis in under a second.

## Checks

dev-2, `nacre-ci/arch-nacre` (rustc 1.99.1, ruff 0.16.10). Rebased on main `c6777550e`: `cargo
fmt --check`, `cargo clippy -p nacre-helper --all-targets -- -D warnings`, `cargo test -p
nacre-helper --lib` (1074 passed; includes the Hyprland capability snapshot and the frozen
CLI/backend references), `tools/generate_compositor_config.py --check`, `test_compositor_config`
and `test_window_routing`. On the previous base `111f3bb` also `tools/compositor_boundary.py`,
ruff format/check on `tools/check.py`'s file set, `luac -p` on `config/hypr/conf/*.lua`, the
window-rule conflict check, `test_bindings`, `test_shell_routes`, and the full `cargo test -p
nacre-helper --no-fail-fast` (48 targets, 1333 passed, 1 failed: `palette_pipeline`, extra
wallpaper-media thumbnails). That test also failed after `--lib` in one container but passes 2/2
on this branch and on main run alone after `cargo build --examples`, so it is order/build
dependent, not this change. Orient `orient-decoder`: fmt, clippy `-D warnings`, unit tests (3,
with the new one) and its integration tests pass.

Desktop re-check of B3/B5 not run: the TEST host failed the swap gate (2.5 GiB zram swap used,
MemAvailable 9.4 GiB, owner session active). The build and the N0318 scenario (per-click
pointer round, then a held-pointer round, Settings/Colors/Widgets/Welcome sizes, overview) are
staged there for a re-run.
