# Arch packaging and setup progress

## Done

- Isolated `feature/arch-packaging-setup` worktree from main `5483399`.
- Inspected source deployment, managed file conflict checks and release snapshots.
- Decision: split package base (`nacre`, optional `nacre-ai`, `nacre-brain`), shared code/native artifacts owned by pacman; per-user setup uses configuration preflight and release snapshots without starting a desktop before login.
- Existing source/live deployment remains supported.

## Next

- Implement mapping, package staging/build, terminal setup and regression tests.
- Validate clean chroot, namcap and fresh test user; update installation/AUR documentation.
- Commit/push milestones, one PR; rebase on current main.

## Open questions

- Orient Arch package name/version contract: coordinate with Orient owner.
- Niri setup API: awaiting backend owner's capability/file-map contract; currently Hyprland only.
- Host lacks devtools/namcap; `sudo -n` requires a password. Asked for isolated build host or scoped tool provisioning. No maintainer configuration changes authorized or made.

## Validation

No package build or fresh-user test performed yet. AUR publication explicitly excluded.
