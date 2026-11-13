# N0411 — Pre-cut fixes and installer hardening (v0.1.0 preview)

Status: implemented on `release/n0411-precut`. Items come from the cut rehearsal (F1-F4) and the installer security review (H1-H9); each is the smallest root-cause change.

| Item | File | Change and why | Test |
|---|---|---|---|
| F1 | `packaging/arch/repository.py` | `fetch_previous` accepts the `nacre-niri` family that `build()` retains; without it the second publication rejects its own previous cut. | `test_package_repository.py`: `fetch_previous` over the built site (patched `urlopen`). |
| F2 | `packaging/arch/repository.py` | Pages title/h1 is "Nacre packages" for a non-dev core version; dev versions keep "Nacre development packages". | Same test: both titles asserted. |
| F3 | `.github/workflows/publish-arch.yml` | `--prerelease` only when `development`, so `arch-0.1.0-1` is not a pre-release beside v0.1.0. | `yaml.safe_load`. |
| F4 | `.github/workflows/arch-packages.yml` | `timeout-minutes` 120 → 180 for the first hosted build with the fork package (comment says to re-bound once measured). | `yaml.safe_load`. |
| H1 | `packaging/arch/bootstrap.sh` | Refuse a key file with more than one `pub:` record before `pacman-key --add` (it imports every key in the file). | `bash -n`; count check exercised against a doubled key file in the container. |
| H9 | `packaging/arch/bootstrap.sh` | If `pacman` (python install or the nacre transaction) fails or is declined after key and repo were added, print the three undo commands (sed the Include line, rm the conf, `pacman-key --delete <fpr>`) and exit 1; nothing is run. | `bash -n`; `undo` output rendered in the container. |
| H2 | `tools/package_transaction.py` | Non-TTY stdin: piped lines answer only `Proceed with installation? [Y/n]`; every other `[Y/n]`/`[y/N]` prompt (key import, removal, conflicts) and EOF get `n`. Interactive path unchanged. | `test_package_transaction.py`: fixture asks an Import PGP key prompt first and asserts `n`, then the transaction gets the piped `y`. |
| H6 | `tools/package_transaction.py` | `sudo LC_ALL=C pacman …` (sudo's own VAR=value form) instead of `env LC_ALL=C sudo pacman`. Not `sudo env …`: the CI delivery users (`publish-arch.yml`, `arch-runtime.yml`) are limited to `NOPASSWD: /usr/bin/pacman` and would be refused; the VAR=value form is unconditional for `ALL` rules and passes `env_check` for restricted ones. | Same suite (fixture `sudo` receives the new argv); sudo behaviour checked in the container. |
| H4 | `crates/nacre-helper/src/aur_review.rs` | Removed the legacy branch mapping a non-`default` profile id to mode `native` (owner 2026-11-12: review on by default; `native` only via the explicit `aurReview` setting). | `cargo test -p nacre-helper --lib aur_review`. |
| H5 | `tools/nacre_modules.py` | `uninstall --purge` unlinks a symlinked own directory instead of `rmtree` through it (rmtree refused the link and failed after other removals had run). | `test_install_modules.py`: symlinked `~/.config/nacre` is unlinked, its target kept. |
| H3 | `docs/installation.md` | Optional 3-line verification of `install-arch.sh` against `install-arch.sh.sig` with `gpgv` and the pinned fingerprint. | `gpgv --keyring` flow checked in the container. |

Deferred (owner/CI decisions, not done here): H7 render `install-arch.sh` from the tested `sourceRevision` or record its digest in `release.json`; H8 protected environment / required reviewer for the signing key in `publish-arch.yml`.

Supersedes the N0409 note that a legacy custom profile stays `native`. No temporary duplicates. Net product line delta: see the unit report.
