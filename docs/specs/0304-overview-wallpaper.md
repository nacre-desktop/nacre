# N0304: Dynamic wallpaper in Niri overview

## Scope

Keep motion wallpaper running while stock niri or nacre-niri has its overview
open. The Nacre wallpaper policy and desktop `bead.desktop.covered` projection
ignore window coverage only during that state. Other pause reasons and all
non-overview behavior remain unchanged; Hyprland has no overview signal and is
unchanged. Desktop widgets animate while visible in the overview.

The Niri compositor topic already publishes `overviewOpen`. Nacre passes that
topic to the wallpaper motion controller, which clears covered/fullscreen hiding
while the overview is open. The shell reuses its `bead.overview.open` store for
the widget covered projection. No Bead or compositor changes, duplicate paths,
or temporary files are introduced.

## Acceptance evidence

- Deterministic projection test: tiled windows cover; opening overview clears
  `covered`/`hidden`; closing it restores coverage.
- Focused `nacre-shell` tests, formatting, clippy and static checks run on dev-1.
- On dev-1, `cargo fmt --check`, Ruff 0.16.10 format/check, and
  `python tools/compositor_boundary.py` passed; the first compile attempt then
  stopped because the exported source omitted the Orient submodule.
- `cargo test -p nacre-shell --bin nacre-shell --locked -j2` completed with
  873 passed, 3 failed, 5 ignored. The three failures were existing wallpaper
  `extras::tests` missing-call results also tracked as N0208 load flakes; each
  exact test passed on its isolated rerun. `cargo clippy -p nacre-shell
  --all-targets --locked -j2 -- -D warnings` passed.
- After rebasing to `996bd345d`, the exact-head shell suite completed with 874
  passed, 2 failed, 5 ignored; the N0304 regression passed. Both failures are
  app-scope tests from N0216 (`a_missing_app_is_started_with_its_page_socket_and_daemon`,
  `widgets_app_receives_its_initial_search_argument`). The first also failed
  when rerun alone. Hosted main run 43298863488 already reports the same two
  failures for `996bd345d`; the N0304 change has not been pushed while main's
  desktop job is red.
- Nested desktop TEST capture: play a short video, cover its output, compare
  consecutive frames while overview is open and while covered outside it; record
  wallpaper-helper CPU in both states. SSH to desktop timed out repeatedly;
  capture and CPU evidence remain pending.
- Desktop widgets also receive `bead.desktop.covered = false` in the overview,
  so visible desktop animations resume there.

## Change size

Product change is limited to the shell/controller projection. No temporary
duplicate remains and no expiry applies.
