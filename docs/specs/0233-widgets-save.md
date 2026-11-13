# N0233 — Widgets app adds never saved; Pill detail duplicate view id

Platform's tier-2 smoke (N0316, row 22, bugs B6 and B7) found two Widgets app bugs.

## B6 (high): "Add to desktop" / "Add to bar" never committed

Symptom: the copy appears under "On your screen", but `settings.get {"store":"widgets"}`
stays rev 0 / exists false and nothing renders on the bar or desktop.

Root cause: the app's edit path in
[`reload.rs`](../../crates/nacre-shell/src/reload.rs) writes a draft to the store only
for ops in its `persistent` list. That list still named `toggle-form`, which B0198
replaced with the non-drag `add` and `remove` ops in
[`behavior.luau`](../../shell/widgets/app/behavior.luau). Every other op (`style`,
`setting`, `undo`, `reset`, `default-trigger`) and every drag drop saved; the two button
ops only updated `widgets.draft`, and closing the app discarded it.

Fix: list `add` and `remove` as persistent (one line). Test
`widgets_app_add_and_remove_commit_the_layout_store` drives the real Widgets app runtime
with a file-backed store: an `add` writes a second clock to the store (rev 1, draft
cleared), a `remove` of that copy writes it away again (rev 2).

## B7 (medium): Pill detail fails to build with a second Pill

Symptom: `Reload failed — old UI kept: bead.kit:405:1: duplicate or too long view id
widgets-app-root…preview-stage.stage-bar.stage-top-bar.right.system-pill`.

Root cause: the Pill widget renders a fixed `system-pill` id. The live bar renderer
([`registry.luau`](../../shell/widgets/registry.luau) `M.renderer`) gives every copy
after the first its own id scope (`widget.<iid>.`); the app's preview bar renderer
([`preview.luau`](../../shell/widgets/app/preview.luau) `render_bar`) did not, so the
second Pill repeated the id of the first.

Fix: the same two-line copy scoping in `render_bar`. Test
`pill_detail_builds_with_two_pill_copies_on_the_bar` loads the real app with the default
layout plus a second Pill and the Pill detail selected, and checks both pills are in the
stage bar.

## Checks

dev-2 (`nacre-ci/arch-nacre:current`, tested on origin/main 87891924a; later main commits up to 1d98a8242 touch only helper, menu and docs): `cargo fmt --check`
(rustfmt 1.10) ok; `cargo clippy -p nacre-shell --all-targets -- -D warnings` ok; whole
`cargo test -p nacre-shell --bin nacre-shell` 912 passed, 1 failed, 4 ignored; the failure
(`colour_role_is_saved_per_widget_instance_and_shared_by_its_forms`) is a pre-existing race:
`model_harness()` in `widget_app_tests.rs` reuses one per-process temp dir for two parallel
tests. It passed alone and with its module (9/9). `tools/check_luau.py` (LUAU_ANALYZE_OK) and
`tools/i18n/check.py` ok. Net delta: product +7 / -1 lines
(`reload.rs` +3/-1, `preview.luau` +4), tests +111.

Out of scope (Platform): Hyprland window sizes (B5), bar clicks on nested Hyprland (B3).
