# N0157 — Retire the pre-split shell archive

Status: Done; full desktop CI and isolated package staging passed.
Migration-only literal-name exception is documented below.

## Scope and decisions

Owner authorization (2026-11-10): remove the previous shell after N0156
(`81c205ab2`). The product is `crates/nacre-shell/` plus `shell/`; the framework
is the exact Bead Git dependency recorded by Cargo.

Delete all of `bead-shell/`, including the expired B0214 historical mirror,
spikes, examples, evidence and retired packaging. The mirror expiry was
“delete 7 days after the Nacre replacement is trusted”; that condition passed.
Delete `docs/development/b0214-import/`: its only external live backlink was the
README archive description, now removed. Git history retains import provenance.
No temporary duplicate is introduced. App-config roots and rollback files remain
out of scope; README changes are limited to its shell layout description.

## Live hooks retained

- Move the unchanged product dependency policy to `crates/nacre-shell/deny.toml`.
- Move the Rust-generated schema/Luau analyzer and wallpaper renderer smoke to
  `tools/check_luau.py` and `tools/wallpaper_video_smoke.py`; preserve native,
  golden and renderer coverage. Retire archive-only Python capture/proof suites
  and the previous product packaging tests; current package tests remain.
- Remove broad archive exceptions in source, compositor and public-name scans.
- Rename the installed private bundle and notices directory to `ui-shell`,
  updating installer, package runtime and tests together.
- Shipped native binaries link Bead (`crates/nacre-shell/Cargo.toml`), so retain
  its complete MIT attribution in `packaging/licenses/LICENSE-Bead-MIT`. Stage Nacre’s
  GPL text directly from root `LICENSE`; remove stale license-boundary prose
  and the duplicate GPL text. Keep Outfit and vendor notices.

## Acceptance

Run every desktop workflow command in an Arch container on the build host, plus
isolated package stage/setup tests proving license contents ship. No desktop
TEST host access or visual captures are needed for this deletion.

Every desktop-job command in `.github/workflows/check.yml` completed in the
Arch CI container (8 CPUs, 16 GiB); no desktop TEST host access was used.

## Remaining references

`git grep -l bead-shell`: only the historical benchmark `provenance.csv` under
`docs/research/bead-vs-qml-shell-benchmark-20261110/` and specs 0054, 0156, 0157.

The N0156 upgrade cleanup still requires the old runtime name: installer,
package runtime and release cleanup, plus their five regression-test files.
These remove old owned paths and preserve custom paths; they never launch the
retired runtime. Removing those guards would leave existing installs uncleaned.
`git grep -il quickshell`: the 39 historical documentation, attribution and
reference-evidence paths listed in N0156, this cleanup record, and these eight
migration-only paths (the explicit exception to the history-only grep target):

- `nacre/shell-tools/install.py`
- `nacre/shell-tools/tests/test_installer_dropin_guard.py`
- `packaging/arch/test-installed.py`
- `tools/package_runtime.py`
- `tools/releases.py`
- `tools/tests/rehearse_bead_recovery.py`
- `tools/tests/test_releases.py`
- `tools/tests/test_setup.py`

## Validation log

The first full workflow attempt stopped at
`settings_pages_tests::current_scene_picker_dispatches_once_on_pointer_release`:
866 shell tests passed, one failed and four were ignored. Its fake daemon hit
`Protocol write failed`, then its call channel closed. The same workspace test
binary passed that exact test independently (1/1 in 0.13 s). No interaction or
harness code was changed. The full workspace rerun passed, including that test:
867 shell tests passed, zero failed and four were ignored. The remaining workflow steps also passed.

The source gate initially required Ruff to reformat the shortened compositor
exclusion expression; after that formatting-only correction, validation resumed
at the failed source step. No Rust or product interaction code changed for either
retry. Evidence is retained in the task report and CI logs.

- `cargo fmt --check`, workspace Clippy with `-D warnings`, examples, workspace
  tests, minimal protocol consumer and release helper build: passed.
- Trace memory-policy tests, no-default-features and all-features checks, and all
  three product binary builds: passed.
- `python tools/check_product_shell.py --analyzer /runner-temp/luau/luau-analyze`:
  passed; 189 Luau modules and all 23 private wallpaper-renderer checks.
- `cargo deny --manifest-path crates/nacre-shell/Cargo.toml --all-features
  --config crates/nacre-shell/deny.toml check`: advisories, bans, licenses and
  sources OK. Pinned Bead MIT notice compared byte-identical to its dependency.
- Translation coverage (1787 messages) and generated assets: passed.
- `dbus-run-session -- xvfb-run -a /tmp/check-env/bin/python tools/check.py`:
  passed; 404 tools, 138 AI, 54 Brain and 376 shell-tools tests (one tools skip),
  plus syntax, lint, compositor and JavaScript checks.
- `python tools/capture-aur-review.py --output /tmp/nacre-aur-review`: passed.
- `cargo test -j2 --locked -p nacre-helper --test daemon memory:: -- --ignored
  --nocapture`: passed (one test).

With `NACRE_HELPER_BINARY`, `NACRE_ORIENT_BINARY` and `PYTHONPATH` set as in the
package-stage-install workflow, this equivalent isolated install/staging command
passed all three tests, including byte-for-byte shipped MIT/GPL license checks:

```sh
/tmp/check-env/bin/python -m unittest \
  test_setup.PackageSetupTests.test_apply_is_offline_repeatable_preserves_private_state_and_rolls_back \
  test_setup.ArchPackageLayoutTests.test_split_packages_have_no_file_collisions_and_ship_native_artifacts \
  test_packaging_bundle
```
