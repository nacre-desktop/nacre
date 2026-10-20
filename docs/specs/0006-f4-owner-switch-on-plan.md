# N0006 — F4 owner switch-on and rollback plan

- ID: N0006
- Repo: nacre-desktop/nacre
- Implementer: Luna docs worker; lead review
- Status: Review — source and vendor install checklists updated for N0014
- Depends on: N0014 and N0013 integrated; owner execution remains owner-controlled
- Date: 2026-10-20
- Scope: docs only

## Goal

Maintain a source-verified owner checklist for enabling F4 on source and package
installs, with activation rollback and physical verification. The executable
steps are in [f4-switch-on.md](../development/f4-switch-on.md); this spec records
plan and status only.

## Scope and disposition

N0014 is the activation-only switch on main; N0013 provides package vendor units.
The owner procedure covers both install types, full source installer deployment,
empty `ready_to_enable`, enable and verification, disable, controller rollback,
TTY recovery from the recorded journal, and manual package downgrade after
disable. Full N10 transactional package/setup rollback is deferred and is not an
activation prerequisite. No publication, installation, live unit change or
physical check is performed by this documentation task.

## Review checklist

- [ ] Verify command names, paths and state transitions against N0014, N0013,
  `tools/f4_switch.py`, `tools/releases.py`, and `nacre/shell-tools/install.py`.
- [ ] Keep this file as plan/status; keep executable steps only in the procedure.
- [ ] Confirm docs-only diff, approved author/committer and `[no-publish]` commit.

## N0014 split decision — 2026-10-20

The owner approved a small activation-only switch instead of making the full
N10 package/setup transaction a prerequisite. N0014 implements ordinary-user
`nacre f4 status|enable|disable|rollback [--transaction ID]`. The procedure
requires the owner to retain the exact verified previous package archive.
Package downgrade follows successful disable and status verification; it is not
part of activation rollback or atomic with setup-file restoration. Full N10
transactional package rollback remains deferred. Package publication,
installation and live switch execution remain owner actions.
