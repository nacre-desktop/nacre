# N0009 — F4 exclusive destination writer handoff

- ID: N0009
- Repo: nacre-desktop/nacre
- Implementer: one fresh Luna worker; coordinator final review
- Status: **Done** — accepted source `ab3bcae4`, exact-head CI `39431539570` green
- Depends on: source8a/F4 candidate for local implementation; N0002 PASS and
  exact-head green F4 remain integration gates
- Date: 2026-10-19

## Goal

Serialize daemon settings ownership and offline profile installation for the same
actual config destination, regardless of socket endpoint. Preserve private staging
and refusal of unsafe files; unblock the ownership prerequisite for N0006/N0010.

## Context and scope

Starting source: prospective F4 integration b003aa8fb0ce095a851bba95ceabf202fd3bf9e2
(source8a). Own isolated worktree, local commits only; preserve frozen capture source.
Read N0001/N0004, foundation/protocol/standing-rules and N0006's switch-on plan.
Relevant files: crates/nacre-helper/src/daemon.rs, daemon/socket.rs, paths.rs and
actual settings publication hook; nacre/shell-tools/profile-policy.py; tools/
releases.py, setup.py, prepare.py; direct profile-writer callers and focused tests.

Endpoint-parent daemon.lock excludes duplicates only in that endpoint folder;
NACRE_DAEMON_SOCKET can select another folder for the same settings destination.
Installers never acquire it. releases.py::deploy installs profile before cutover;
deploy_package calls profiles.install(HOME). release.lock covers installers only.
Profile JSON/Lua publish separately. Runtime Paths honors absolute XDG_CONFIG_HOME;
profile install uses HOME/.config. Lock actual destinations; retain path policy.
Recheck these facts on the resolved starting commit before implementation.

Allowed: bounded lease integration, actual publication hook and focused tests.
No dependency/protocol/schema/capability, installer redesign, notification ownership,
AI-policy/paru or persistent opt-in change; N0010 owns managed activation/rollback.

## Steps

1. Introduce Rust `WriterLease::acquire(config_dir)` and Python
   `writer_lease(config_dir)` context manager with identical Linux flock contract:
   `<actual-nacre-config-dir>/.writer.lock`, exclusive nonblocking, held by open fd.
   Preserve endpoint lock separately. Daemon acquires destination lease BEFORE
   settings load/provider startup or listener activation, retains it until every
   writer task stops. Both ordinary and inherited-socket startup use this path.
2. Installer acquires before reading destination profile/snapshot or any write;
   retains through both profile publications and failure restoration. Deploy,
   package setup and direct profile-policy CLI/import calls must enforce it.
   Outer transactions pass a validated lease object in-process; the existing
   deploy subprocess may use an explicitly inherited fd validated against the
   exact destination inode/lock. Never trust a boolean/env bypass or reacquire
   a separate flock while already holding it. Order: release.lock then writer
   lease; direct profile install takes writer lease only. Daemon takes no release.lock.
3. Installer target remains its actual HOME-relative destination; explicit
   `install(home)` stays isolated from ambient XDG variables. Runtime uses Paths.
   Same actual config directory means one lease regardless of socket/runtime
   directory; different XDG config directories may run concurrently. Inventory
   prepare.py's empty-stage writes and any restore path; acquire the stage's own
   lease for profile publication, never the owner's HOME lease accidentally.
4. Walk absolute paths component-by-component from a pinned `/` directory fd,
   using no-follow directory opens and fd-relative lookup/creation throughout.
   Reject links, non-directories and group/other-writable ancestry, except a
   root-owned 01777 sticky directory (for example `/tmp`) immediately containing
   a pre-existing caller-owned 0700 private anchor. Pin and validate that anchor
   before creating anything below it; reject foreign/missing/unsafe anchors.
   Trusted root-owned safe ancestors (including `/` and `/home`) are permitted;
   private descendants, immediate config root and lock belong to this user.
   Existing safe modes stay unchanged; new dirs 0700. Lock is regular, own UID,
   0600, nlink=1, O_NOFOLLOW and CLOEXEC except explicit transfer. Validate fd/path
   inode identity, including inherited fd. Revalidate the complete pinned chain
   and lock before each creation/publication, including settings flushes; fail
   closed on replaced/renamed ancestry. All writes remain relative to pinned fds,
   never a freshly resolved pathname: a replacement cannot redirect a write into
   a foreign tree. A last-instant rename may leave writes only in the pinned
   caller-owned tree; detect lost reachability and report failure, not success.
   Touch the actual publication hook only as needed; no general library rewrite.
   Never unlink/replace/truncate a lease file, chmod/chown an existing owner file,
   or snapshot/restore the lock.
5. Lease busy/unsafe means fail BEFORE mutation, with sanitized destination-busy
   error; no waiting, daemon stop, retry, active-service probe or forced takeover.
   Existing stopped-session installer precondition remains: legacy QML writers
   must be quiesced by the owner; this cooperative lease does not fence old binaries,
   external editors or uncooperative writers. Do not claim an online hot upgrade.
   Interrupted JSON/Lua publication reports failure; preserve/restore under lease,
   and stop if restoration cannot prove consistency. No success on partial state.

AC/evidence: deterministic private barrier tests exercise real daemon startup
(normal and activated) versus direct install, deploy/package publication and
restore; both acquisition orders and read-before-write exclusion. Prove same
config/different endpoints exclude, different config roots coexist, explicit
private HOME ignores ambient XDG, fd inheritance releases on exit/crash, and
partial publication cannot race a new daemon. Cover symlink/hardlink, wrong
mode/UID, replaced path/inode and foreign-file refusal with unchanged contents;
prove root-owned sticky `/tmp` plus own 0700 anchor succeeds, while nonsticky
writable ancestry, foreign/unsafe anchors and symlinks fail before any foreign
write. Barrier-test anchor replacement between validation and publication: no
write reaches the replacement tree and the operation reports failure. Use
fixtures/mocks for unavailable foreign UID, identifying that limit. Retain
existing single-writer/durability and frozen parity expectations. Run focused
Rust/Python checks first; broaden only for touched behavior on an explicitly
free correctness slot. No performance rerun merely for installer changes.

## Stop, review and owner boundary

Stop for unguarded writer, ambiguous destination, lease/security mismatch,
required profile/schema migration, widened privilege/dependency or broader scope.
Report exact gap; do not bypass it. Return exact commit/files, named AC outcomes,
limits and cleanup; Sol review plus independent security review precede integration.
Coordinator alone marks Ready after the dependency gate and worker/correctness
slot assignment. This draft grants no implementation or measurement.
No build/test/CI/live probe or publication/install/unit change was run for this draft.
Owner approval remains required for package publication/install and live execution.
Commit using approved identity, docs(spec):, [no-publish] and [skip ci] for this draft.

## Owner implementation grant — 2026-10-19 afternoon

This supersedes the draft-only/blocked wording above: implement now using Luna,
local commits only, no push/live operations. Coordinator final review required.
Private builds/tests serialize before tonight’s N0002 window; request the heavy
slot with exact revision/argv, do not self-start. No profiling. Integrate only
after F4 is green. Report commits, AC evidence and unsupported cases honestly.

## Settings state-root clarification — 2026-10-19 implementation review

Actor stores include state-root documents (notably writable battery-alerts),
not just config-root documents. Daemon must pin AND nonblocking-lease both actual
Paths.config and Paths.state before startup, deduplicated by inode and acquired
in stable bytewise absolute-path order; release all on any failure. Installer
profile publication leases its actual config destination. Every settings read/
publication selects only its schema-authorized exact pinned parent, never a
basename redirected into config or an arbitrary caller path. Different config
roots can coexist only when their other writer roots are disjoint; shared state
roots exclude concurrent daemons even with different config/socket roots. Tests
cover state-root durability, shared-state exclusion and fully disjoint roots.
This fixes N9 only; frozen N2 source8a remains unchanged. N10 must include actual
writer roots in its quiescence lease set when restoring those destinations.


## Completion — 2026-10-20

Accepted integrated source: `ab3bcae4ff3e6c126bb8d5efb1636f5196c79c41`.
[Exact-head Source checks 39431539570](https://github.com/nacre-desktop/nacre/actions/runs/39431539570)
passed both desktop and fresh-install jobs. F4 dependency source `9b37c663`
already passed exact-head CI39402420054. Coordinator review and the independent
security/evidence review are complete; N9 is Done.

The hosted run passed native fmt/Clippy/tests/examples/release and consumer
checks, 405 tools tests (one skip), 116 AI, 45 brain, 459 shell-tool tests,
534 immutable parity cases with zero failures, and sandbox integrity checks.
The frozen profile-install reference is unchanged: the paired compatibility
check permits only its intended new 0700 directories and empty 0600 writer
lock, while preserving all profile contents and unrelated records. New tests
also prove existing safe directory modes are not changed. Symlink target reads
pin the inode and reject replacement/metadata races; focused writer/profile
suites passed 17/10 cases and the lead independently verified them.

This accepts cooperative destination ownership, not an online hot upgrade.
Legacy QML and uncooperative writers still require quiescence. No package publication/install, installed file, daemon unit or live session
was changed. Final full checks ran on GitHub; the earlier private correctness
receipt remains separate and makes no performance claim.
N10's complete managed switch/rollback remains a separate unfinished gate.
