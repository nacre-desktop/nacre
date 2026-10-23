# N0040 — Launcher groups, visiting workspaces and output-agnostic window focus

Status: **Implemented; pending lead review and exact-head CI; unpublished**. Consumer: Bead B0037
(`docs/specs/0037-workspace-launcher-parity.md` on Bead). All changes are additive; QML is unchanged.

## 1. `menu.groups` (version 1, class `desktop`, outside safe mode)

Params `{}` (no fields). Result `{catalog_rev, groups:[{id, label, icon}]}`. Groups are the
top-level `kind:"group"` catalogue rows (`home, apps, actions, style, setup, software, health,
learn, power`) in the QML rail's default order (`nacre-menu` `model::rail` with no preferences;
`home` first). `label` is localized like `menu.query` rows (the context catalogue is localized at
load with the shared N0035 language), `icon` is a Material symbol name (may be empty). `id` is a
valid `menu.query` `route`. Hiding and user ordering (`launcher.rail.order/hidden`) stay with the
caller. `catalog_rev` changes when the catalogue reloads; re-query then. Types:
`nacre_protocol::menu::{MenuGroups, MenuGroup, MenuGroupsResult}`; generated
`protocol/schema/menu-groups*.json`, `protocol/luau/menu.d.luau`, golden `menu-groups.jsonl`.

## 2. Visiting workspaces in the `compositor` topic

Workspace rows (`workspaces[id]`, and the copies embedded in `windows[].workspace` and
`outputs[].activeWorkspace`) gain optional `originOutput` (string), `originIndex` (integer >= 1)
and `visiting` (bool, `originOutput != outputId`). Source: niri only, the origin ledger
`state/workspace-session.json` written by the niri workspace-policy session, read on each topic
publish and used only when its `socket` equals the daemon's niri socket. Fields are absent for a
row without a ledger entry and always absent on Hyprland (no visiting concept, nothing invented).
Consumers must treat missing `visiting` as false. `fixedSlot` is not published: it is derived
from the niri workspace `name` (named workspaces) which is already in the row.
The topic has no generated schema; this section is its contract for these fields.

## 3. `window.focus {window, output?}`

N0031 already published `window.focus {output, window}`. `output` is now optional: when omitted
the window is looked up by its published ID alone and focused on its own output (output-agnostic,
what the bar glyph needs). When given it is a guard: the window must be on that output or the call
fails `validation`. Same validation, permission class (`desktop`), safe-mode refusal and
`{changed}` result as `workspace.activate`. A window without a known output (Hyprland window not
mapped to a monitor) fails closed. Schema `protocol/schema/window-focus.json` regenerated; golden
`shell-actions.jsonl` gains an output-less call.

## Temporary duplicates

None added.
