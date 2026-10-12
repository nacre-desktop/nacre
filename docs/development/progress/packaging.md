# Arch packaging and setup progress

## Done

- Worktree: `/home/siverteh/.local/share/nacre/ai/worktrees/nacre-arch-packaging`; branch `feature/arch-packages-setup`; one draft PR: https://github.com/nacre-desktop/nacre/pull/8.
- Rebased on integrated main `fa37a28` (PR2/3/4); source/live installation retains compositor facade reload and rollback behavior.
- Split Arch package base: core `nacre`, optional `nacre-ai`, `nacre-brain`; separately owned dependency `python-orient-colors`. One distro dependency mapping in `packaging/packages.json` and generated conventional PKGBUILD.
- Native Dolphin/Thunar integrations and offline font/reference palette/lock assets built at package build time. Shared code/native assets under `/usr`; private config and managed runtime/service links deployed by setup.
- Terminal keyboard/profile/component selection, plan/confirmation/apply/summary; `--non-interactive`, `--plan`, `--yes`. Existing private keyboard/profile/palette/accounts preserved; package transaction supports failed-apply rollback without a live desktop.
- Ten packaging/setup regressions, including split-file ownership, manifest validation and seeded palette privacy. Local tools suite passes; changed Python Ruff checks pass.
- Installation and maintainer/AUR documentation written. AUR submission explicitly excluded; compiler-free installs require binary distribution, not merely an AUR recipe.
- Orient canonical dev5 sdist SHA256 `58ec261d8c0d5888796ff46775f243dbfa6a11253ea9fa4f97bc65c57aba5b8b`: native stock-Arch clean chroot passes 57 tests + wheel smoke. Artifact `/var/tmp/nacre-orient-build/python-orient-colors-0.1.0dev5-1-any.pkg.tar.zst`; log `/tmp/nacre-orient-final-chroot.log`; namcap has one Python entry-point parser warning, no errors.
- First Nacre build found source-helper import path omission; fixed. Native full check then found missing writable XDG_RUNTIME_DIR for nspawn builduser, not helper regression; recipe now creates private 0700 runtime for tests. Python/QML/Lua/compositor checks and tools/AI/Brain suites pass in current chroot run; shell native tests still running.
- Disposable runtime-only Arch root bootstrapped separately at `/var/tmp/nacre-runtime-test/root`; no maintainer configuration/account changed.
- User approved scoped local pre-push fix: exclude fetched public origin/main as well as old remote task head. Fixture confirms unpublished retired-identity commits still rejected; original guard backed up privately. No public repository hook/policy changes.

## Next

- Commit/push final setup preservation/launch-path fixes; rebuild exact candidate with clean `makechrootpkg -c` and final namcap review.
- Install final packages into disposable runtime root; test fresh ordinary user without gcc/npm/Ruff, plan/apply/repeat/conflict/rollback and CLI imports.
- Update this note with final revisions, package hashes and all check outcomes; refresh PR and CI; keep one PR.

## Open questions / limits

- Temporarily stacked Orient pin + startup engine fingerprint from PR6; rebase/drop once final dependency bump is merged.
- Niri owner supplies public generator/capability APIs; setup_ready remains false until complete initial config/keyboard/profile/session deployment is available. This package advertises Hyprland only; no niri backend/session files edited here.
- Cold-login graphical behavior is not yet verified; headless setup checks do not prove a rendered desktop or keyboard input.

## Build / test state

Stock Arch chroot `/var/tmp/nacre-packaging-chroot/root`, isolated official mirror `/tmp/nacre-arch-pacman.conf`, separate cache `/var/tmp/nacre-packaging-cache`. Host's personalized mirror gave 404/signature failures; verification remained enabled and host pacman config stayed unchanged.

Long build runs in root tmux session `nacre-arch-packaging`, log `/tmp/nacre-build-chroot.log`. Generated candidate recipe `/var/tmp/nacre-build/PKGBUILD`; generate from exact pushed commit via `packaging/arch/recipe.py`, not a moving branch. Package chroot success and fresh-user acceptance remain pending until final evidence is recorded here.
