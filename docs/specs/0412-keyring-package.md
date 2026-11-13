# N0412 — `nacre-keyring`: deliver a renewed signing key through updates

Status: implemented on `release/n0412-keyring`. Fixes security review C1.

## Problem

The repository key `886CBE272BA30B44DBB55F66714CFAAC876C349E`
(`packaging/keys/nacre-repo.asc`) has a certify-only primary key that expires
2028-10-13 and one signing subkey, `3430416234A8409E510D95AAC5379CE9AD3D8B4F`,
that expires **2027-10-14**. `install-arch.sh` imports the key once. Before this
change nothing delivered a renewed key, so on 2027-10-15 `pacman -Syu` would fail
for every installed system.

## Mechanism

- **`nacre-keyring`** is a new split package of the `nacre` package base (arch
  `any`, GPL-3.0-only, no dependencies, same version as Core). `stage.py`
  (component `keyring`) generates its files at package time from
  `packaging/keys`, so the key has one source of truth:
  - `/usr/share/pacman/keyrings/nacre.gpg`: a copy of `nacre-repo.asc` (armored,
    which `gpg --import` accepts);
  - `nacre-trusted`: `<nacre-repo.fingerprint>:4:`;
  - `nacre-revoked`: empty.
- **`packaging/arch/nacre-keyring.install`** follows the archlinux-keyring
  pattern: `post_install`/`post_upgrade` run `pacman-key --populate nacre` when
  pacman's keyring is initialized. The populate step re-imports the certificate,
  which merges new self-signatures (a later expiry or a new subkey), and lsigns
  the trusted fingerprint if it is not lsigned yet. `recipe.py` writes this file
  beside the PKGBUILD it renders, because makepkg reads `install=` from the
  PKGBUILD's directory.
- **Core depends on `nacre-keyring`.** The dependency is only in the rendered
  `package_nacre()`. It is not in `packages.json`, because that list is also
  setup's dependency list for source installs and the base's build-time
  `depends`. Every Core cut has a new version, so every update re-runs populate.
- **The bootstrap does not change.** The first import has to stay manual,
  because pacman cannot verify the repository before it trusts the key. After
  that, the keyring package keeps the key current. Uninstall leaves the key in
  pacman's keyring, as with archlinux-keyring. The bootstrap's undo text already
  prints `pacman-key --delete`.
- **Admission (`repository.py`).** `verify_artifacts` requires `nacre-keyring`
  with exactly Core's version, plus Core's dependency on it. `fetch_previous`
  accepts the retained family.
- **Expiry guard.** `check_signing_expiry` runs on the publish path
  (`repository.py candidate --output …`). It makes a probe signature with the
  same `--local-user` to find the key gpg actually signs with. It then reads
  that key's expiry, bounded by the primary's expiry, from the shipped
  `nacre-repo.asc`, because users verify against that copy. It refuses to
  publish within 90 days of expiry, or when the signer is missing from the
  shipped key. It is not called inside `build()`, because the
  `arch-runtime.yml` delivery diagnostic signs with an ephemeral one-day key
  that is not part of the shipped certificate. Signing itself is unchanged.
- **CI:** the `pacman -U` lists in `arch-packages.yml` and `arch-runtime.yml`
  include the keyring archive. Without it, Core's dependency cannot be resolved
  offline.

Pacman verifies every package signature in a transaction before it installs
anything. A release that delivers a renewed key must therefore be signed by a
subkey that users' current copy still accepts. The runbook below follows from
that rule.

Tests: `test_package_repository.py` (keyring admission and version tie; the
expiry guard with a throwaway certify-only primary and a 30-day signing subkey,
covering renewing only the CI copy, renewing the shipped copy, primary expiry
and an unknown signer); `test_setup.py` (the recipe declares the package,
`install=`, `arch=any` and Core's dependency; staged keyring files match
`packaging/keys`; no split collisions). `test-installed.py` checks the keyring
files' ownership and `pacman-key --list-keys <trusted fpr>`. `test-delivery.py`
checks that the signed repository delivers `nacre-keyring` with Core.

There are no temporary duplicates. Not covered: a change of primary key (new
fingerprint). That needs a reviewed bootstrap fingerprint change and a
transition release that ships both keys.

## Owner runbook: renew before 2027-10-14

Deadline: from **2027-07-16** (90 days before 2027-10-14) publication refuses
to sign until the shipped key is renewed. Start by **June 2027**. Steps marked
**[OWNER]** need the offline primary key or the repository secrets. Nothing
secret is ever committed: only the public export goes into
`packaging/keys/nacre-repo.asc`.

### A. Extend the existing subkey (recommended)

1. **[OWNER]** On the offline machine that holds the primary key, extend the
   primary and the signing subkey (choose your own durations):
   `gpg --quick-set-expire 886CBE272BA30B44DBB55F66714CFAAC876C349E 2y` and
   `gpg --quick-set-expire 886CBE272BA30B44DBB55F66714CFAAC876C349E 1y 3430416234A8409E510D95AAC5379CE9AD3D8B4F`.
2. **[OWNER]** Export only the public key:
   `gpg --armor --export 886CBE272BA30B44DBB55F66714CFAAC876C349E > nacre-repo.asc`.
   Check `gpg --show-keys --with-colons nacre-repo.asc`: there is one `pub`, the
   fingerprint is unchanged and the new expiry dates are shown. The lead
   commits the file over `packaging/keys/nacre-repo.asc`. The fingerprint file
   and `bootstrap.sh` stay unchanged.
3. Publish a normal release. CI still signs with its current subkey copy, which
   users accept until 2027-10-14. The expiry guard passes because it reads the
   renewed shipped key. On `pacman -Syu` / `nacre update`, `nacre-keyring`'s
   `post_upgrade` imports the new expiry. The site's `nacre-repo.asc` serves new
   installs.
4. **[OWNER]** Before 2027-10-14, rotate the CI secret. The secret still holds
   the old expiry, so gpg in CI refuses to sign with it after that date. Export
   the subkey only (`gpg --armor --export-secret-subkeys 3430416234A8409E510D95AAC5379CE9AD3D8B4F!`),
   then `gh secret set ARCH_REPO_SIGNING_KEY -R nacre-desktop/nacre` from that
   file, then delete the file securely.

### B. Replace the subkey (only if it is compromised, or to change algorithm)

1. **[OWNER]** Add the new subkey on the offline primary:
   `gpg --quick-add-key 886CBE272BA30B44DBB55F66714CFAAC876C349E ed25519 sign 1y`.
2. **[OWNER]** Export the public key as in A.2. It now contains both subkeys.
   The lead commits it.
3. Publish a release **still signed by the old subkey**: leave the CI secret
   unchanged. This must happen more than 90 days before the old subkey expires,
   because the guard checks the old subkey. Users receive the new subkey through
   `nacre-keyring`.
4. Wait at least one release cycle so that installed systems update.
5. **[OWNER]** Rotate `ARCH_REPO_SIGNING_KEY` to the new subkey only
   (`--export-secret-subkeys <new subkey fpr>!`). If the old subkey is
   compromised, revoke it on the primary, export again and publish once more.

### Systems that missed the window

A system that did not update before the old expiry cannot verify new releases.
To recover, download `https://nacre-desktop.github.io/nacre/nacre-repo.asc` and
check that `gpg --show-keys` shows `886CBE272BA30B44DBB55F66714CFAAC876C349E`.
Then run `sudo pacman-key --add nacre-repo.asc` and `sudo pacman -Syu`. The
primary key is unchanged, so its existing lsign still applies.
