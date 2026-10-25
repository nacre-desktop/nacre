# N0077 — Dashboard pending package list

Status: Implemented; pending lead review. Date: 2026-10-25.

`dashboard.updates` adds `packages: [{name, old, new, repo}]` and
`truncated: boolean`. Each item describes a pending package version transition;
`repo` is the repository prefix returned by `checkupdates`/`pacman -Qu`, or
`aur` for the existing `paru -Qua`/`yay -Qua` command. The list is bounded to
500 rows, while `count` and `text` retain their previous all-row count. A
truncated list is exactly 500 entries. Both fields deserialize with empty/false
defaults for older publishers. The generated JSON Schema and Luau types are
derived from `UpdatesState`; generated transcript coverage uses realistic
official-repository and AUR records.

The daemon parses only the output of the package commands N0055 already selects.
It adds no command, Flatpak query, network request, consent setting, timer,
cache write or updater behavior. N0055's 30-minute interval, refresh method,
safe-mode and offload rules, namespace routing, exit handling and shared lock
remain unchanged. Flatpak is absent from the existing dashboard counter; the
explicit update flow and `paru -Syu --skipreview` are untouched. A malformed
nonblank package row makes the refresh fail rather than publish partial details.

Validation: producer tests cover checkupdates repository rows, paru AUR rows,
empty output, command/parse errors and the 500-row cap. Protocol tests cover the
generated fixture, repo values and backwards-compatible omitted fields. Full
desktop CI is required in the task's remote Arch container. No temporary
duplicate or generated-only source is introduced.
