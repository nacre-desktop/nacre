# Arch packaging and setup progress

## Done

- One PR: https://github.com/nacre-desktop/nacre/pull/8; branch `feature/arch-packages-setup`. Task worktree `nacre-arch-packaging` in the workflow worktree store; private handoff records exact local paths.
- Implementation tested from `fc9da959cf390f22d68b6e5cb06017c746dccfbd`, rebased repeatedly (latest tested base `d08e06d`). Final docs/recipe metadata commit does not change tested implementation. Dependency PR6 is merged; no temporary dependency stack remains.
- Split package base: `nacre` (native x86_64 core), `nacre-ai` and `nacre-brain` (optional architecture-independent code). Orient owns `python-orient-colors`; exact release/tag `6ee1784`, version `0.1.0dev5-1` agreed with its maintainer.
- Packages own shared code/fonts/native integrations under `/usr`. `nacre setup` deploys managed private configuration, wrappers and runtime/service references through existing configuration preflight and release snapshot/rollback. Source/live deployment keeps its live gates and source-runtime byte snapshots.
- Terminal keyboard/compositor/profile/AI/Brain choices, plan/confirmation/apply/summary; non-interactive `--plan`/`--yes` flags. Existing private keyboard, profile access preference, palettes, accounts and notes preserved. Hyprland now; incomplete niri setup stays hidden using backend/package readiness.
- One distro package mapping, generated conventional PKGBUILD, pinned font checksums/licenses, Arch compiler/linker flags. Native integrations built before package installation; no user gcc/npm/Ruff/provisioning requirement.
- Thirteen packaging/setup regressions cover conflicts, invalid manifests, file/link ownership, shared template permissions, failed apply rollback, repeat setup, optional choices, private preferences and rollback→reapply. Installation/optional-app docs, source contributor path and maintainer AUR steps updated.
- AUR publishing not attempted. No maintainer desktop/user configuration or account changed. Test tooling installed separately; isolated stock-Arch build/runtime roots used. User-approved local pre-push scope correction retains unpublished-identity rejection; it is outside the PR and backed up privately.

## Validation

| Gate | Evidence / result |
| --- | --- |
| Clean Arch build | `makechrootpkg -c`, stock repositories, exact candidate + separately built Orient package; all three runtime packages and automatic debug package produced |
| Full source check | `tools/check.py`: lint/format, Python/JSON/shell/Lua, QML formatting, compositor contracts, palette CLI and Brain JavaScript pass |
| Tests | tools 121 + AI 100 + Brain 36 + shell 372 = **629 passed** |
| Orient | Canonical dev5 sdist clean chroot: 57 tests + installed-wheel smoke pass |
| Runtime namcap | **0 errors**; 39 Python local/optional import, 21 Quickshell local-import and 47 config/subprocess dependency warnings reviewed; see maintainer guide |
| Debug package | All four build-ID links resolve with core installed; namcap's isolated debug-archive missing-runtime diagnostics are cross-package pointers |
| Fresh user, all components | Unmodified final archives: plan has no config side effects; apply, CLI imports, keyboard/profile/unit/runtime links, repeat preservation, conflict refusal, rollback and reapply pass |
| Terminal wizard | Real TTY: keyboard/profile/component prompts, cancellation, apply and `--with ai` overridden by terminal No pass |
| Core-only | AI/Brain physically removed: core setup, missing optional-package refusal, repeat/rollback/reapply pass |
| No build tools | Runtime root lacks gcc/cc/g++, Node/npm, Ruff, Git and pip |
| CI | Required desktop/source check passed for implementation candidate; final docs/recipe commit checked separately |

Candidate hashes (archives and logs retained privately with task artifacts):

| Archive | SHA-256 |
| --- | --- |
| `nacre-0.1.0dev2-1-x86_64.pkg.tar.zst` | `1b20431b108d2c655723814ddd596bf60485d48146314e0456864310315838df` |
| `nacre-ai-0.1.0dev2-1-any.pkg.tar.zst` | `b4246f942966e34f9506bd357790357a928d88fc81d94f8f8c5123cef2edf254` |
| `nacre-brain-0.1.0dev2-1-any.pkg.tar.zst` | `cfd3cf474233a22b21b1a7acb0524834b5d4d2c5c197b491214b4a6e2fb2c412` |
| `nacre-debug-0.1.0dev2-1-x86_64.pkg.tar.zst` | `c93e1340594fc9430535b87aae6c354744deb877d63dc420bdebcb9acbe6f8ae` |
| `python-orient-colors-0.1.0dev5-1-any.pkg.tar.zst` | `03896d20cc8c48fcf4326285780c75346c4c6c9da0eb7182e72ddad57906a54a` |

## Next

- Review/merge the single PR. Before release or AUR submission, regenerate the recipe from the exact tagged release source and rebuild if source/dependencies differ from this tested pin.
- The maintainer performs AUR account/SSH publication following `packaging/arch/README.md`; distributing signed binaries is a separate step for compiler-free users.

## Open questions / limits

- Cold graphical login/hardware behavior remains unverified; headless setup and native widget rendering are not a live Wayland login.
- Niri's owner retains `setup_ready=false` until its complete initial config/keyboard/profile/session contract is ready. No niri backend/session files changed here.
- Package rollback restores private files/references, not a pacman upgrade; keep previous archives for coordinated downgrade.

## Operational lessons

- Shared package templates must be readable (0644/0755); copying generated private 0600 defaults unchanged makes fresh-user planning fail.
- Chroot Qt tests need a private 0700 XDG_RUNTIME_DIR and closed stdin; otherwise Quickshell cannot create its runtime or interactive failure tests wait for Enter.
- Validate file/link ownership before snapshot/apply. Pacman-owned palette references remain system links after rollback, while source runtimes retain byte snapshots.
- Keep concurrent graphical chroot checks separate/serial; an intermediate overlapping run lost its X display. Final acceptance was rebuilt serially with all checks enabled.
