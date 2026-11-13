# N0405b — User docs landing

Reviewed and landed the Platform lead's 2026-11-13 documentation drafts after
checking every command, key, path and claim against this tree (5cb5e22a2).

Landed: new `README.md`; `docs/first-boot.md`, `keybindings.md`, `theming.md`,
`widgets-and-settings.md`, `switching.md`, `troubleshooting.md`, linked from
`docs/README.md`. Fixed: the dead `installation.md#modules-what-setup-touches`
anchor in `usage.md`; Bead's licence in `architecture/structure-and-licensing.md`
(MIT, per Bead's `LICENSE`); the stale "Phase 0" `shell/README.md`;
`recovery.md` no longer assumes Hyprland is installed (`wayland-wm@*` listing,
`nacre session` from a TTY); `optional-apps.md` installs `nacre-ai`/`nacre-brain`
with `pacman -S`. Orient and nacre-niri got their pages in their own repos.

Dropped or changed from the drafts: broken `media/*` image links (the shots do
not exist yet; they stay as HTML comments); links to a Bead widget tutorial that
is not on Bead main (now the `examples/plugins/hello` example and spec 0203);
pearl scroll/click, "show window symbols", launcher Health entry, lock-screen
palette and hyprlock/Clasp claims that no source confirms; Hyprland-tier
feature gaps are now the ones `compatibility.md` lists. `installation.md` and
`compatibility.md` were left to their own units; the README status line still
says "not yet released" and must change at the cut.
