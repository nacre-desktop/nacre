# Arch packaging and setup progress

## Done

- Isolated `feature/arch-packaging-setup` worktree from main `5483399`.
- Inspected source deployment, managed file conflict checks and release snapshots.
- Decision: split package base (`nacre`, optional `nacre-ai`, `nacre-brain`), shared code/native artifacts owned by pacman; per-user setup uses configuration preflight and release snapshots without starting a desktop before login.
- Existing source/live deployment remains supported.

- Implemented package runtime file/link planning, terminal setup and offline release transaction with failure rollback.
- Added 8 setup regressions covering conflicts, rollback, repeated setup, keyboard/palette preservation and unavailable optional packages.
- Tools suite: 80 tests passed; changed Python files pass Ruff format/lint.
- Stock Arch chroot created using official mirror and separate cache. Orient candidate f96fecd builds there (57 tests + wheel smoke); namcap has one entry-point parser warning, no errors.
- Added package build/staging/recipe generation and installation/AUR maintainer documentation.

## Next

- Implement mapping, package staging/build, terminal setup and regression tests.
- Validate clean chroot, namcap and fresh test user; update installation/AUR documentation.
- Commit/push milestones, one PR; rebase on current main.

## Open questions

- Orient Arch package name/version contract: coordinate with Orient owner.
- Niri setup API: awaiting backend owner's capability/file-map contract; currently Hyprland only.
- Host lacks devtools/namcap; `sudo -n` requires a password. Asked for isolated build host or scoped tool provisioning. No maintainer configuration changes authorized or made.

## Validation

Orient dependency built in clean chroot. Nacre package build and fresh-user verification remain pending. Local full CI cannot use Xvfb (not installed); complete native checks will run inside package chroot. AUR publication explicitly excluded.
