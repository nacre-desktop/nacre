# N0200 — Launcher cleanup

## Goal

Remove launcher destinations that cannot work, hide optional and compositor-specific rows when unavailable, and consolidate duplicate entries before tidying the rail.

## What changed

- Routed dictation setup through the existing terminal action helper; removed the duplicate All apps action and the redundant wallpaper picker route (the Wallpapers provider remains).
- Hidden AI/Brain branded rows when their capability is absent, omitted Brain results without the Brain capability, and hid visitor-workspace actions outside Niri.
- Scoped Hyprland help to Hyprland, added the Niri wiki row, and removed the AI/Brain prefix hint chips.
- `shell/command-catalog.json` remains: `crates/nacre-shell/src/commands.rs` embeds it in production. `shell/shell-routes.json` remains: production route-coverage tests embed it.

## Non-working rows found and fixes

| ID | Problem | Fix |
| --- | --- | --- |
| `actions.dictation.setup` | Invoked Kitty directly while checking only the `voxtype` capability. | Use the existing launcher terminal action wrapper. |
| `actions.apps` | Duplicated the All apps rail destination. | Removed. |
| `style.wallpapers.picker` | Duplicated the Wallpapers provider destination. | Removed; keep the provider. |
| `actions.bring-visitors` | Presented disabled outside Niri. | Hide outside Niri. |
| `learn.hyprland` | Shown on all compositor tiers. | Show only on Hyprland. |
| Brain search rows | Source preference was checked without the Brain capability. | Require both. |

The static audit counted 147 entries (operation counts recorded in the N0200 task report); a complete target-by-target pass over all 147 entries, 87 software rows, 12 font rows, and provider outputs has not yet established a verified-working total.

## Checks

- `python -m json.tool nacre/shell-tools/launcher-menu.json`: passed after editing.
- `git diff --check`: passed.
- Rust, Luau, launcher test suites, golden regeneration, and the full dev-1 desktop CI replica remain to be run.
