# N0135 — Brain git history (U1)

Status: done; acceptance and the complete desktop CI job passed.

The vault owns a private git repository. `nacre-brain git init --remote URL`
creates a baseline import, committed ignores and a pre-commit guard. The guard
checks indexed bytes, forbidden paths, symlinks, size and provider secret rules;
diagnostics contain only filenames and rule names. Large raw files remain local
and receive SHA-256, size and origin entries in `raw/MANIFEST.md`. The `vault:`
origin identifies a file imported from the existing vault; it does not invent
provenance for an original external source.

Native and retained Python note, memory and maintenance writers share the vault
lock followed by capture.lock. Writes enqueue relative paths in ignored state;
the daemon batches them after a two-minute quiet period. Git commits use a
separate index. N0137 extends selection to all non-ignored vault changes while
retaining queued attribution. Pushes target origin main without
force; recovery rebases only task-owned vault commits and aborts conflicts.
`nacre-brain git status --json` exposes commit, push, pending paths and errors.
The sync timer is retired from installation and maintenance actions.

Acceptance: idempotent baseline, forbidden staged paths, every secret rule,
symlinks and oversized files; concurrent writers batch without data loss; local
bare remote recovery and conflict handling; installer unit retirement. Run helper
checks, touched Python suites and the complete desktop CI job in an isolated Arch
container. No live vault, live units, viewer changes or remote Brain pushes.

Validation passed with temporary vaults and local bare remotes: baseline commit
hash stays unchanged on repeated init; all nine secret rules and protected paths,
symlinks and size limits block; the installed hook rejects secrets without printing
values. Twenty writes from two processes batch within two commits without lost
files; Python/native memory captures share deduplication. Rejected pushes rebase
owned commits and conflicting edits abort with readable status. The full desktop
job passed in one isolated Arch container (8 CPUs, 16 GiB), including workspace
Clippy/tests, all 535 helper parity cases, Python suites, product proofs, native
galleries and CPU raster memory regression. Worktree evidence: `.lead/ci.log` and
`.lead/acceptance.py`.

Temporary duplicates: retained Python `.py.rollback` tools and their
`nacre-brain-queue.py` bridge support existing compatibility/parity consumers;
delete 7 days after the native replacement is
trusted and those consumers have migrated. The legacy sync binary remains while
`brain_data::api` and the helper dispatcher expose it, and the Python maintenance
compatibility tool imports its rollback implementation; delete 7 days after
those interfaces are retired. Sync unit templates are deleted, and fresh installs
do not install sync units.
