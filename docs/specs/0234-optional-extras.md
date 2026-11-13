# N0234 — No AI/Brain degrades calmly; Hyprland overview without window capture

Platform's tier-2 smoke (N0316, rows 12, 19, 20 and 24, bugs B2 and B1) found two shell bugs.

## B2 (high): Super+A and Settings > AI/Brain without the optional packages

Symptom: with `nacre-ai`/`nacre-brain` not installed (daemon `capabilities` flags `ai=false`,
`brain=false`), the launcher rows and the dashboard card hide, but Super+A and `nacre left`
open the full AI sidebar with "Request failed", and `nacre settings nacre-ai` / `brain` open
full pages of controls that cannot work.

Root cause: the sidebar ([`chat.rs`](../../crates/nacre-shell/src/chat.rs)) never read the
capability flags, so it called `ai.launcher.list` and showed the failure. Settings already
hides the `nacre-ai`, `brain` and `panels/ai-sidebar` pages from navigation by their
`visibleWhen` (and shows the Add-ons page with "Nacre AI: install" / "Brain: install" rows),
but `Settings::page_for` opened any page asked for by id, hidden or not.

Fix (existing flags, no new probes):

- The sidebar leases the `capabilities` topic with its panel. Published flags without
  `ai: true` set `chat.missing`; then `ready()` is false for every method (no calls), and
  [`chat/main.luau`](../../shell/chat/main.luau) shows only the header (logo, title, pin)
  and a centred "Nacre AI is not installed" with the hint "To add it, install the nacre-ai
  package and run Nacre setup." (two new strings). Unknown flags (topic not yet published)
  keep today's sidebar.
- `page_for` sends a page its capability hides to the first shown page of its group: the
  add-on pages open Add-ons (the established not-installed state), `panels/ai-sidebar` opens
  Panels. The same rule applies to the other capability-hidden pages (Bluetooth, Power), so a
  route can no longer reach a page the navigation hides. Capability flags are now refreshed
  before the page is resolved.

Tests: `missing_ai_shows_the_not_installed_state_and_calls_nothing` (fake daemon: no call
from the launcher or quick-chat section, header without providers, new golden
`chat-missing-{1,1.5}.png`); Settings capability tests assert `brain` → `addons` with
brain=false, `nacre-ai` → `addons` and `panels/ai-sidebar` → `panels` with ai=false. The
Settings visual is the existing `settings-addons-1.png` golden (the redirected page is that
page). One fixture gained `powerIdle: true` because it opened Power with flags that hide it.

## B1 (high on Hyprland): overview window previews

Symptom: with `windowCapture=true` the Hyprland Windows overview leased live previews; every
capture PNG failed with "asset not in this generation's snapshot", a "Reload failed — old UI
kept" banner and a 26 s slide stall. Platform sets `windowCapture=false` on Hyprland for v0.1.

Shell fix: the overview leased captures whenever the Wayland capture protocols existed,
ignoring the backend bit (Settings already hides the live-previews toggle without it).
[`extras.rs`](../../crates/nacre-shell/src/extras.rs) now leases only when the `compositor`
topic advertises `capabilities.windowCapture`; otherwise tiles keep their app icons and no
capture is requested. The preview test first checks windowCapture=false (open: no lease, no
`preview` field), then the existing lease/thumbnail path with the bit set.

Root cause of the rejected assets (for a later Bead/Shell unit, not fixed here): the tile
renders the capture with `ui.Image{path=item.preview}`
([`shell/extras/main.luau:51-52`](../../shell/extras/main.luau)). A `path` image must be a
file of the view's bundle snapshot: bead-ui `generation.rs:856-869` (Bead rev a6fbc10f)
joins it to the view root and rejects anything not in `self.assets`, and the hub's PNGs live
in `$XDG_RUNTIME_DIR/bead-capture-<pid>/wN-M.png` (new name per frame). The wallpaper picker
shows runtime files with `icon=` instead (`shell/extras/wallpaper/main.luau:115`, resolved as
`@icon:` and exempt from the check). Switching the tile to `icon=` needs a check that the
icon/texture cache is bounded for a new path every 350 ms per window.

## Checks

dev-2 (`nacre-ci/arch-nacre:current`, rustfmt 1.10.0) on origin/main 50e95460e (Bead
a6fbc10f) plus this unit: `cargo fmt --check` ok; `cargo clippy -p nacre-shell --all-targets
-- -D warnings` ok; whole `cargo test -p nacre-shell --bin nacre-shell` 914 passed, 0 failed,
4 ignored (an earlier run caught the missing `chat.missing` store declaration in
`chat/behavior.luau`: 10 chat runtime/route tests); `tools/check_product_shell.py`
LUAU_ANALYZE_OK (183 modules), its nested-sway native step cannot start in the container;
`tools/i18n/check.py` and `tools/compositor_boundary.py` ok. No helper or Python change.
The new `chat-missing` goldens were looked at (1x and 1.5x): same header, colours and type as
`chat-launcher`, no provider pills, footer or search.

Net delta: product +58 / -10 lines (`chat.rs` +23, `settings.rs` +14/-5, `extras.rs` +3/-1,
`popouts.rs` +1, `chat/main.luau` +12/-3, `chat/behavior.luau` +1/-1, two strings in
`shell/i18n`), tests +65 / -2, two goldens.

Not done: the sidebar's footer Brain button with AI installed but Brain absent still calls
`ai.launcher.action brain` (Brain alone missing was not part of the smoke rows). No nested or
live capture of the new states (the smoke harness is Platform's); a Platform tier-2 re-run of
rows 12, 19, 20 and 24 is the end-to-end check.
