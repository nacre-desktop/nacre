# N0137 — Brain sweep commits

Status: implemented; validation results are recorded in the unit report.

The daemon sweeps every non-ignored Git change after a two-minute quiet period,
including notes written by older installed tools and edits made outside Nacre.
Candidates come from `git status --porcelain=v2 -z`; renames include both paths.
The queue continues to supply agent attribution. Commits say `brain: N files
(sweep)` with an `Agent:` trailer when queued metadata exists.

Both vault locks cover candidate validation and separate-index staging. Files
modified within ten seconds wait for a later cycle. Each candidate passes the
existing forbidden-path, symlink, size and secret checks. Unsafe files are left
local and reported as `{path, rule}` pairs in status `blocked`; safe changes can
commit in the same batch. The owner's staged index entries remain untouched.
Push/rebase recovery retains N0135's existing ownership and conflict rules.

Acceptance uses temporary vaults and local bare remotes: unqueued addition,
owner modification, deletion/rename, mixed safe/secret files, fresh-file deferral,
quiet-period debounce and preservation of staged work. Run formatting, Clippy,
helper tests, `.lead/acceptance.py` and the complete desktop CI job in one dev-1
Arch container named `n0137`, eight CPUs, 16 GiB, nice 10, Cargo -j2, tmux,
and `/root` mode 711. No live vault writes, installation or live units.

This supersedes N0135's queue-only selection; it adds no compatibility copies.
