# Arch packages

One package base builds `nacre` (core), `nacre-ai`, `nacre-brain`,
`nacre-session-niri` and `nacre-session-hyprland`. Orient owns
its independent `orient` package; do not bundle Orient into Nacre.
The tested exact engine revision/version is in `dependencies.lock.json`.
Development Arch snapshots use `0.1.0devN.rCOUNT.gREVISION`, with an increasing
Git commit count so pacman recognizes each tested update. These sort below the
corresponding stable `0.1.0`; Python uses
`0.1.0.devN` and Git tags use `v0.1.0-dev.N`.

`packaging/packages.json` is the authoritative distro package-name mapping.
`packaging/sessions.json` declares compositor adapters, dependency groups and
login entries; setup and package staging use that same registry. A new backend
adds its integration module and dependency group here, plus plan/apply/rollback
and installed-runtime tests. A new OS adds a package-manager adapter without
putting distro-specific transactions into session integration code.
`recipe.py` renders a conventional PKGBUILD with explicit dependency arrays;
edit the mapping, regenerate and review the recipe rather than maintaining
several lists. Native integrations are compiled by `build.py`, using Arch
compiler/linker flags. Fonts are pinned makepkg sources with checksums and
license notices. `stage.py` installs only below `$pkgdir/usr`; no package hook
writes to a user's home, starts services or downloads accounts/models.

Shared code is under `/usr/share/nacre`, native app-scoped modules under
`/usr/lib/nacre`, and fonts under `/usr/share/fonts/nacre`. Setup deploys private
configuration/wrappers plus references to shared code through `configure.py`
and the existing `releases.py` snapshot/rollback owner. Reference palette and
lock assets seed only missing user files; later palette changes are preserved.
Setup does not run source checks, provision a virtual environment or invoke
source/native installers. Source contributors retain `install.sh`.

## Prepare a candidate

Commit and push the reviewed source first. Generate a recipe in a separate,
empty build directory pinned to that exact commit (not a moving main branch):

```sh
python packaging/arch/recipe.py --revision HEAD --output /tmp/nacre-build/PKGBUILD
cd /tmp/nacre-build
makepkg --printsrcinfo > .SRCINFO
namcap PKGBUILD
```

Git sources are fixed commits; the Orient submodule is initialized from the
separately fetched exact pin with file transport enabled only for that command.
`SKIP` is used only for the fixed VCS sources; font assets have SHA-256 checksums.
Checks include the repository's lint/format/tests, QML and Brain JavaScript.

## Clean chroot and fresh-user proof

Follow the Arch [clean-chroot guide](https://wiki.archlinux.org/title/DeveloperWiki:Building_in_a_clean_chroot)
and [packaging guidelines](https://wiki.archlinux.org/title/Arch_package_guidelines).
Use stock Arch repositories, not a maintainer's personalized pacman config:

```sh
sudo pacman -S --needed devtools namcap
sudo mkdir -p /var/tmp/nacre-chroot
sudo mkarchroot -C /usr/share/devtools/pacman.conf.d/extra.conf \
  /var/tmp/nacre-chroot/root base-devel
# Build Orient in its owner's package directory first, then:
makechrootpkg -c -r /var/tmp/nacre-chroot \
  -I /path/to/orient-0.1.0dev5-1-x86_64.pkg.tar.zst
namcap PKGBUILD ./*.pkg.tar.zst
```

Record exact commits, package hashes and check summaries in the release
validation record, following this [clean-chroot checklist](#clean-chroot-and-fresh-user-proof). Review every namcap diagnostic; Python
entry-point module detection can produce false positives, which require wheel
smoke evidence rather than suppression.

Use a separate VM or disposable chroot with **runtime packages only**, a fresh
ordinary user, no provider credentials and no inherited desktop configuration.
As that user, run setup `--plan`, then `--yes`; verify keyboard/profile, executable
wrappers, runtime references, service units and `nacre status`. Repeat setup,
change a seeded palette and confirm it remains unchanged; create a conflicting
managed edit and confirm setup refuses it. Verify rollback and injected apply
failure through tests. Check that gcc, npm and Ruff are absent from the runtime
root. The reproducible all-component smoke test is
`python /path/to/packaging/arch/test-installed.py --compositor niri`, copied into the runtime root
and run as a fresh `nacre-package-test*` user. It refuses existing Nacre config
or credential directories. Repeat with separate users for `hyprland` and `both`;
the package workflow performs all three in a runtime-only Arch image. Test the terminal wizard through a real TTY too;
confirm cancellation creates no configuration, then confirm apply. Cold-login graphical validation needs a VM/Wayland session and is a
separate gate; headless configuration success is not proof of desktop rendering.

## Tested development publication

`Pinned Arch packages` runs after successful Source CI for the latest main
revision. Its default Core build fetches the independently released, signed
Orient archive, verifies its source/version/recipe against Nacre's lock, and
injects it into a clean stock-Arch chroot. No replacement Orient archive is
rebuilt under that version. A manual `use_released_orient=false` run is available
for dependency development; that run cannot publish to the binary repository.
The optional Niri companion remains a separately pinned build.

`Publish tested Arch repository` admits only successful package runs from this
repository's main branch and requires passing Source CI for the exact compiled
source. It checks archive recipe digests, split versions and all three installed
setup proofs. Signing uses a separate ephemeral GnuPG home and only the Actions
signing subkey. Package and database signatures are required by the public
installer. Source checks, namespace probes and installed tests are not optional.

Before publication, the delivery job installs Core through the signed pacman
database in a disposable Arch environment. Niri is initially absent; setup must
install its dependencies, validate configuration, preserve a plain Niri fixture
and succeed on repeat setup. This checks package-manager behavior as well as
archive contents. Hardware login/unlock/display/screen-sharing acceptance is
still separate from this headless proof.

Passing snapshots publish immutable GitHub preview assets and an atomic Pages
repository. The database selects the current versions; previous package files
are retained to allow downloads already in progress to finish. The immutable
assets include exact PKGBUILD/`.SRCINFO`, package checksums/signatures and source
receipt. Use `PACKAGE-SHA256SUMS` for the files in that release; the site's
`SHA256SUMS` additionally covers its database and retained archives. The public
installation/update path is in [the installation guide](../../docs/installation.md).
The first Nacre repository cut is still being validated; check the package index
before treating it as available.

To validate a cut manually:

```sh
gh workflow run arch-packages.yml --repo nacre-desktop/nacre --ref main \
  -f component=core -f use_released_orient=true
# Recovery only: promote the SAME successful, source-checked package run.
gh workflow run publish-arch.yml --repo nacre-desktop/nacre --ref main -f run_id=RUN_ID
```

An existing package version cannot be replaced with different archive bytes.
Publication rejects older cuts, mismatched recipes, a different Orient engine,
missing runtime proofs and foreign/fork workflow runs. Repair a failed check and
produce a new tested cut instead of changing signature or sandbox policy.

## Signing maintenance

The pinned public key/fingerprint are in `packaging/keys`; bootstrap has the same
fingerprint, checked by a regression test. Back up the certification key and
revocation certificate in a secure offline/OS key store. GitHub receives only an
expiring signing subkey as `ARCH_REPO_SIGNING_KEY`, never the certification key.
No key values belong in commands, logs or Git history.

Track signing-subkey and certificate expiry. Renew or replace the signing subkey
before expiry, publish its updated public certificate, and arrange trusted-key
refresh for existing installations before retiring the old signing key. A
primary-key change also requires a deliberately reviewed bootstrap fingerprint
change and trust transition. Revocation/recovery must not disable pacman
signature checks. Stable-channel promotion and AUR publication are separate
maintainer actions; this workflow currently publishes development previews.

## AUR publishing — maintainer action only

No AUR repository is created or pushed by this task. After clean-chroot and
fresh-user/desktop checks pass:

1. Confirm the `nacre` package-base name is available and agree the release pin
   with Orient's maintainer. Publish its tested dependency first.
2. Use the maintainer's own AUR account and SSH key. Follow the
   [AUR submission guide](https://wiki.archlinux.org/title/AUR_submission_guidelines).
3. Regenerate the release PKGBUILD from the tagged, tested source revision.
   Review the dependency versions and source hashes. Build again in a clean
   chroot if any source or dependency changed.
4. Clone `ssh://aur@aur.archlinux.org/nacre.git` into a separate directory.
   Copy only PKGBUILD and any required recipe support files; this recipe has
   no install hooks or patches. Run `makepkg --printsrcinfo > .SRCINFO` there.
5. Run `namcap PKGBUILD` and inspect `.SRCINFO`, then commit and push those two
   files to the AUR. The single package base exposes all five split packages.
   Do not push package binaries, caches, private profiles or build artifacts.
6. For compiler-free end-user installs, separately distribute tested signed
   binaries through an Arch repository or release artifacts. AUR submission
   alone still asks users/AUR helpers to build packages.

AUR registration is currently paused. Binary distribution can proceed independently
through a signed pacman repository. Never copy another account's authentication
to publish.

## Niri companion

`nacre-niri` is a separate package base, so building or installing core Nacre
does not require Rust. Its recipe pins both the Nacre patch revision and official
Niri26.04 commit. Runtime/build dependency names remain in `packages.json`.
Generate it from a reviewed, published Nacre commit:

```sh
python packaging/arch/niri/recipe.py --revision HEAD --output /tmp/nacre-niri-build/PKGBUILD
cd /tmp/nacre-niri-build
makepkg --printsrcinfo > .SRCINFO
makepkg
namcap PKGBUILD ./*.pkg.tar.zst
```

Use the clean-chroot process above for release builds. All normal Niri features
are retained, including D-Bus/systemd and screencasting. `check()` runs the native
protocol regression test; before distributing the archive, extract it to a private
directory and run `tools/tests/verify_niri_overview_hover.py --binary
/path/to/extracted/usr/lib/nacre/niri/niri --output /path/to/package-hover-proof.json`
from a Wayland desktop. This separately verifies the *packaged* binary against
the current production title bar, real pointer input and legacy stream clients.
It does not verify cold login or hardware-specific behavior.

The recipe disables makepkg's C/C++ LTO injection: GCC LTO objects in PipeWire's
C shims cannot be linked by Rust's `lld`. Niri's own Rust thin-LTO, default
features and ordinary strip/debug processing stay enabled.

The package owns only `/usr/lib/nacre/niri`, `/usr/bin/nacre-niri` and its license/
patch notices. It depends on distribution Niri for its existing resources and
session support. No system Niri binary, session entry or per-user configuration
is overwritten. The source-prepared Nacre Niri session selects this companion first; package setup
currently uses stock Niri;
`nacre-niri` also runs it directly for nested testing. Stock Niri remains available.
Pacman-owned files are validated using their owner, permissions, pinned source,
protocol and executable version. Private source builds instead require matching
binary/patch hashes and nested verification proof. Arch's normal strip/debug split
remains enabled and does not invalidate the system receipt.

Installation makes hover metadata available to every user running the companion;
the current Nacre shell negotiates it once per connection, with stock-stream
fallback and `NACRE_NIRI_HOVER_STREAM=0` as an explicit opt-out. Do not advertise
the full Niri setup wizard until its separate setup contract is ready. Publishing
this recipe does not publish an AUR package or host signed binaries.

For a fresh prepared session, `prepare --stock-niri` persists an explicit stock
selection even if the companion is installed. Existing prepared sessions can
disable the companion by creating `~/.local/share/nacre/niri-on-pc/session/stock-niri`
before the next login; removing that marker restores automatic companion selection.

## Namcap diagnostic review

The runtime packages must have no unresolved errors. Script/config-launched
programs can be reported as unused dependencies; retain dependencies evidenced
by the installed entry points, Lua config and subprocess calls. Quickshell's
`qs.*` modules are local config imports, and Python helpers under the shared
`tools`/`shell-tools` directories use explicit local import paths. These are
not missing distro packages. The Orient imports and CLI are covered by an
installed-package smoke test.

`claude_agent_sdk` and `fastembed` belong to optional, separately provisioned
provider/semantic runtimes; package setup does not install providers/models.
Do not silence a genuinely required dependency as an optional feature.

With Arch's default debug option, makepkg also emits `nacre-debug`. Namcap can
flag its build-ID symlinks to native modules shipped in `nacre`, because those
runtime files are outside the debug archive. Verify both packages installed
together and resolve every link in the combined root. The three runtime
packages are the user installation path; keep Arch's debug generation enabled.

## Installation verification details

Shared package templates must remain readable by fresh users (0644 for files,
0755 for directories and executables). Verify managed file and symlink ownership
before snapshot/apply. Rollback restores private files and runtime references;
pacman-owned references remain system links rather than copied private runtimes.
A package downgrade needs the previous archives as well as setup rollback.

Native Qt checks need a private 0700 `XDG_RUNTIME_DIR`. Run graphical chroot checks
serially and close stdin for non-interactive suites; a competing display or an
unanswered failure prompt is not a test result. Archive extraction and nested
rendering do not replace runtime-only fresh-user and cold-login validation.
