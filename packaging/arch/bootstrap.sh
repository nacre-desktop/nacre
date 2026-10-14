#!/usr/bin/env bash
# Download this file and review it before running. Run as the desktop user.
set -euo pipefail
repository=https://nacre-desktop.github.io/nacre
fingerprint=886CBE272BA30B44DBB55F66714CFAAC876C349E
include='Include = /etc/pacman.d/nacre-dev.conf'
if [[ ${1:-} == --plan ]]; then
  printf 'Trust package key %s\nAdd signed repository %s/arch/dev/$arch\nInstall nacre with pacman -Syu, then run nacre setup as your user.\n' "$fingerprint" "$repository"
  exit 0
fi
if [[ $# != 0 || $EUID == 0 ]]; then
  echo 'Run bash install-arch.sh as your ordinary desktop user (or use --plan).' >&2
  exit 1
fi
if [[ $(uname -m) != x86_64 ]] || ! command -v pacman >/dev/null; then
  echo 'This installer currently supports x86_64 Arch and CachyOS.' >&2
  exit 1
fi
for command in curl gpg sudo pacman-key; do
  command -v "$command" >/dev/null || { echo "Install $command first." >&2; exit 1; }
done
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -m 700 "$work/gnupg"
cat > "$work/nacre-dev.conf" <<'CONF'
[nacre-dev]
SigLevel = Required DatabaseRequired TrustedOnly
Server = https://nacre-desktop.github.io/nacre/arch/dev/$arch
CONF
if [[ -e /etc/pacman.d/nacre-dev.conf ]] && [[ $(sha256sum "$work/nacre-dev.conf" | cut -d " " -f 1) != $(sha256sum /etc/pacman.d/nacre-dev.conf | cut -d " " -f 1) ]]; then
  echo 'Existing /etc/pacman.d/nacre-dev.conf differs; review it before continuing.' >&2
  exit 1
fi
if pacman-conf --repo-list | grep -Fxq nacre-dev && ! grep -Fxq "$include" /etc/pacman.conf; then
  echo 'An existing nacre-dev repository is configured elsewhere; review it first.' >&2
  exit 1
fi
curl --fail --show-error --silent --location --proto '=https' "$repository/nacre-repo.asc" --output "$work/nacre-repo.asc"
actual=$(gpg --homedir "$work/gnupg" --batch --with-colons --show-keys "$work/nacre-repo.asc" | awk -F: '$1=="fpr" {print $10; exit}')
[[ $actual == "$fingerprint" ]] || { echo 'Package signing fingerprint mismatch.' >&2; exit 1; }
sudo pacman-key --add "$work/nacre-repo.asc"
sudo pacman-key --lsign-key "$fingerprint"
sudo install -m 644 "$work/nacre-dev.conf" /etc/pacman.d/nacre-dev.conf
if ! grep -Fxq "$include" /etc/pacman.conf; then
  sudo cp -a /etc/pacman.conf "/etc/pacman.conf.nacre-backup-$(date -u +%Y%m%dT%H%M%SZ)"
  printf '\n%s\n' "$include" | sudo tee -a /etc/pacman.conf >/dev/null
fi
if ! command -v python3 >/dev/null; then
  sudo pacman -Syu --needed python
fi
# The publisher embeds the same provider-aware transaction code used by setup.
python3 -c @NACRE_PACMAN_FRONTEND@ nacre
exec nacre setup
