# Nacre protocol v1 fixtures

GPL-3.0-only; generated Nacre contract data retained in the GPL application.
These protocol-v1 fixtures were originally copied from published Nacre
`6abe2498a86fc5ce93662f98849c6fc24f06207f`. B0134 copies its
`wallpaper-catalogue.jsonl` (optional identity-pinned video poster predecode).
The current workspace dependency is tracked by `crates/nacre-shell/Cargo.toml`
and `Cargo.lock`; refresh the fixtures when their producer contracts change.
B0136 copies its
`wallpaper-active.jsonl` (manual-navigation readiness) and
`core-dashboard-updates.jsonl` (the published bounded package list).

Tests compare exact JSON wire objects with the pinned producer exports before
replay where applicable. Older compatibility captures are retained deliberately.
Object key order is not a wire contract; array order and fields remain checked.
These contracts do not imply runtime service availability. No protocol dependency
or fixture is embedded in a Bead framework crate.
