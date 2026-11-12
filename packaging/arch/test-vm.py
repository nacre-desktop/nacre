#!/usr/bin/env python3
"""Fresh Arch tty1 gate; run under a resource slot with QEMU/KVM and virgl.

Archives become a guest-only signed repository; --repo accepts a published root.
"""

import argparse
import fcntl
import json
from pathlib import Path
import posixpath
import shlex
import signal
import socket
import subprocess
import time
from urllib.request import urlopen

from repository import ROOT, render_bootstrap

PUBLIC = "https://nacre-desktop.github.io/nacre"
FINGERPRINT = "886CBE272BA30B44DBB55F66714CFAAC876C349E"

QEMU = """nice -n 10 qemu-system-x86_64 -machine q35,accel=kvm
    -cpu host -smp 4 -m 6144 -device virtio-net-pci,netdev=net0
    -device virtio-rng-pci -device virtio-vga-gl
    -display egl-headless,rendernode=/dev/dri/renderD128 -rtc base=utc -monitor none"""

INVENTORY = r"""
import hashlib, json, os, stat
from pathlib import Path
result = {}
for root in ('/etc', '/home/tester', '/usr/share/nacre', '/usr/lib/nacre'):
    for folder, dirs, files in os.walk(root):
        for name in dirs + files:
            p = Path(folder) / name
            s = p.lstat()
            value = [s.st_mode, s.st_uid, s.st_gid]
            if p.is_symlink():
                value.append(os.readlink(p))
            elif stat.S_ISREG(s.st_mode):
                value.append(hashlib.sha256(p.read_bytes()).hexdigest())
            result[str(p)] = value
print(json.dumps(result, sort_keys=True))
"""


def run(command, *args, **kwargs):
    return subprocess.check_output(
        [*shlex.split(command), *map(str, args)],
        text=True,
        stderr=subprocess.STDOUT,
        **kwargs,
    )


def leftovers(before, after, managed=()):
    """Gate Nacre survivors; retain the complete dependency/config diff as evidence."""
    changed = {p: v for p, v in after.items() if before.get(p) != v}
    bad = []
    for path, value in changed.items():
        product = "nacre" in path.lower() or Path(path).is_relative_to(
            "/home/tester/.local/state/bead"
        )
        repo = path == "/etc/pacman.d/nacre-dev.conf"
        backup = path.startswith("/etc/pacman.conf.nacre-backup-")
        if path.startswith("/etc/") and (repo or backup or not product):
            continue
        target = str(value[3]) if len(value) > 3 else ""
        target = posixpath.normpath(str(Path(path).parent / target))
        if (
            path in managed
            or product
            or target.startswith(("/usr/share/nacre", "/usr/lib/nacre"))
        ):
            bad.append(path)
    return changed, bad


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--tier", choices=("nacre-niri", "niri", "hyprland"), required=True
    )
    source = parser.add_mutually_exclusive_group(required=True)
    source.add_argument("--packages", type=Path)
    source.add_argument("--repo")
    parser.add_argument("--base", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--lock", type=Path, required=True)
    args = parser.parse_args()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    rows = []
    vm = None
    stage, started = "prepare", time.monotonic()

    def guest(command, timeout=300, input=None):
        result = run(
            *ssh,
            "bash -o pipefail -ec " + shlex.quote(command),
            timeout=timeout,
            input=input,
        )
        with (out / (stage + ".log")).open("a") as stream:
            stream.write(result)
        return result

    def copy(source, destination, download=False):
        remote = "tester@127.0.0.1:"
        run(
            "scp -q -P",
            port,
            *options,
            remote + source if download else str(source),
            str(destination) if download else remote + destination,
            timeout=180,
        )

    def finish(name=None):
        nonlocal stage, started
        rows.append((stage, "PASS", time.monotonic() - started))
        stage, started = name, time.monotonic()

    def inventory(label):
        data = json.loads(guest("sudo python -c " + shlex.quote(INVENTORY)))
        (out / (label + ".json")).write_text(json.dumps(data, indent=2))
        (out / (label + "-packages.txt")).write_text(guest("pacman -Qq"))
        return data

    def interrupted(signum, frame):
        raise RuntimeError("Interrupted by signal " + str(signum))

    for sig in (signal.SIGTERM, signal.SIGINT):
        signal.signal(sig, interrupted)
    with args.lock.open("a") as lock, (out / "qemu.log").open("w") as log:
        fcntl.flock(lock, fcntl.LOCK_EX)
        try:
            key = out / "ssh-key"
            run("ssh-keygen -q -t ed25519 -N '' -f", key)
            seed = out / "user-data"
            seed.write_text(f"""#cloud-config
users:
  - name: tester
    groups: [wheel]
    shell: /bin/bash
    sudo: ["ALL=(ALL) NOPASSWD:ALL"]
    ssh_authorized_keys:
      - {key.with_suffix(".pub").read_text().strip()}
ssh_pwauth: false
bootcmd:
  - [systemctl, enable, sshd]
""")
            (out / "meta-data").write_text(f"instance-id: n0401-{args.tier}\n")
            run("cloud-localds", out / "seed.iso", seed, out / "meta-data")
            base, disk = args.base.resolve(), out / "overlay.qcow2"
            run("qemu-img create -f qcow2 -F qcow2 -b", base, disk, "20G")
            with socket.socket() as sock:
                sock.bind(("127.0.0.1", 0))
                port = sock.getsockname()[1]
            options = shlex.split(
                "-o BatchMode=yes -o StrictHostKeyChecking=no -o ConnectTimeout=3"
            )
            hosts = f"UserKnownHostsFile={out}/known-hosts"
            options += ["-i", str(key), "-o", hosts]
            ssh = ["ssh", "-p", str(port), *options, "tester@127.0.0.1"]
            q = shlex.quote
            command = f"""{QEMU}
 -drive {q(f"file={out}/overlay.qcow2,if=virtio,format=qcow2")}
 -drive {q(f"file={out}/seed.iso,media=cdrom,readonly=on")}
 -netdev user,id=net0,hostfwd=tcp:127.0.0.1:{port}-:22
 -serial {q(f"file:{out}/serial.log")}
"""
            vm = subprocess.Popen(
                shlex.split(command), stdout=log, stderr=subprocess.STDOUT
            )
            (out / "vm.pid").write_text(str(vm.pid))
            deadline = time.monotonic() + 300
            while True:
                if vm.poll() is not None:
                    raise RuntimeError("QEMU exited; see qemu.log")
                try:
                    guest("cloud-init status --wait", timeout=15)
                    break
                except (subprocess.SubprocessError, TimeoutError) as error:
                    if time.monotonic() > deadline:
                        raise RuntimeError(
                            "SSH/cloud-init deadline exceeded"
                        ) from error
                    time.sleep(5)
            finish("inventory")
            # Harness fixtures live outside HOME, apart from this pre-baselined profile.
            guest(
                f"printf '%s\\n' 'if [ -e /tmp/start-session ] && [ \"$(tty)\" = /dev/tty1 ]; then exec nacre session {args.tier}; fi' > ~/.bash_profile"
            )
            guest("""sudo mkdir -p /etc/systemd/system/getty@tty1.service.d
printf '[Service]\\nExecStart=\\nExecStart=-/usr/bin/agetty --autologin tester --noclear %%I linux\\n' | sudo tee /etc/systemd/system/getty@tty1.service.d/vm-gate.conf
""")
            guest("""sudo sed -i -E 's/^#?ParallelDownloads.*/ParallelDownloads = 1/' /etc/pacman.conf
printf '%s\\n' 'Server = https://mirror.osbeck.com/archlinux/$repo/os/$arch' 'Server = https://geo.mirror.pkgbuild.com/$repo/os/$arch' | sudo tee /etc/pacman.d/mirrorlist
""")
            before = inventory("before")
            finish("repository")
            url = args.repo.rstrip("/") if args.repo else "http://127.0.0.1:8080"
            if args.packages:
                archives = sorted(args.packages.resolve().rglob("*.pkg.tar.zst"))
                if not archives:
                    raise ValueError("No package archives")
                guest(
                    "mkdir -p /tmp/repository/arch/dev/x86_64 /tmp/repo-key; chmod 700 /tmp/repo-key"
                )
                for archive in archives:
                    if not archive.name.startswith(
                        ("nacre-debug-", "python-orient-colors-")
                    ):
                        copy(archive, "/tmp/repository/arch/dev/x86_64/")
                fingerprint = guest("""export GNUPGHOME=/tmp/repo-key
 gpg --batch --pinentry-mode loopback --passphrase '' --quick-generate-key 'VM gate <vm@example.invalid>' ed25519 sign 1d
 gpg --batch --with-colons --list-secret-keys | awk -F: '$1=="fpr" {print $10; exit}'
""").splitlines()[-1]
                guest("""set -e
export GNUPGHOME=/tmp/repo-key
 gpg --armor --export > /tmp/repository/nacre-repo.asc
 cd /tmp/repository/arch/dev/x86_64
 for p in *.pkg.tar.zst; do gpg --batch --detach-sign "$p"; done
 repo-add --sign nacre-dev.db.tar.gz *.pkg.tar.zst
 nohup python -m http.server 8080 --bind 127.0.0.1 --directory /tmp/repository </dev/null >/tmp/repo-http.log 2>&1 & echo $! >/tmp/repo-http.pid
""")
                bootstrap = render_bootstrap(ROOT).replace(FINGERPRINT, fingerprint)
            else:
                with urlopen(url + "/install-arch.sh", timeout=60) as response:
                    bootstrap = response.read().decode()
            bootstrap = bootstrap.replace(PUBLIC, url)
            if url.startswith("http://"):
                bootstrap = bootstrap.replace("--proto '=https'", "--proto '=http'")
            guest("cat > /tmp/install-arch.sh", input=bootstrap)
            finish("bootstrap-plan")
            guest("bash /tmp/install-arch.sh --plan")
            finish("bootstrap-install")
            # The final interactive setup refuses stdin; only that known exit is accepted.
            guest(
                "rc=0; printf 'y\\n' | bash /tmp/install-arch.sh >/tmp/bootstrap.log 2>&1 || rc=$?; "
                "cat /tmp/bootstrap.log; [ $rc -eq 0 ] || { [ $rc -eq 2 ] && "
                "grep -q 'non-interactive setup requires --plan or --yes' /tmp/bootstrap.log; }; pacman -Q nacre",
                timeout=600,
            )
            finish("setup")
            guest(
                f"printf 'y\\n' | nacre setup --non-interactive --profile default --layout us --compositor {args.tier} --yes --install-deps",
                timeout=600,
            )
            managed = [
                "/home/tester/" + line.split("\t")[1].removeprefix("~/")
                for line in guest("nacre status --files").splitlines()
                if "\t~/" in line
            ]
            finish("session")
            guest("""touch /tmp/start-session
sudo systemctl daemon-reload; sudo systemctl restart getty@tty1.service
for i in $(seq 60); do systemctl --user is-active --quiet nacre-shell.service && break; sleep 2; done
systemctl --user is-active nacre-shell.service
""")
            time.sleep(10)
            environment = 'export XDG_RUNTIME_DIR=/run/user/$(id -u); eval "$(systemctl --user show-environment | sed "s/^/export /" )"; '
            command = (
                "hyprctl -j monitors"
                if args.tier == "hyprland"
                else args.tier + " msg --json outputs"
            )
            outputs = json.loads(guest(environment + command))
            if not outputs:
                raise RuntimeError("Empty compositor outputs")
            journal = guest("journalctl --user -b --no-pager")
            if "UI_ERROR" in journal:
                raise RuntimeError("UI_ERROR in user journal")
            guest(environment + "grim /tmp/screenshot.png")
            copy("/tmp/screenshot.png", out / "screenshot.png", download=True)
            guest(
                "python -c \"from PIL import Image,ImageStat; v=ImageStat.Stat(Image.open('/tmp/screenshot.png').convert('RGB')).var; print(v); assert sum(v)/3 > 1.0\""
            )
            guest("systemctl --user is-active nacre-shell.service")
            finish("update")
            update = guest(
                "if nacre --help 2>&1 | grep -qw update; then nacre update; else echo 'SKIP: update absent in candidate'; fi"
            )
            finish("uninstall")
            if "SKIP: update absent" in update:
                rows[-1] = (rows[-1][0], "SKIP", rows[-1][2])
            guest(
                "rm /tmp/start-session; uwsm stop; sudo systemctl stop getty@tty1.service; "
                "nacre uninstall --plan; nacre uninstall --purge --yes; systemctl --user daemon-reload"
            )
            finish("package-removal")
            guest(
                "pacman -Qq | grep -E '^(nacre($|-)|orient$)' | xargs -r sudo pacman -Rns --noconfirm",
                timeout=300,
            )
            guest(
                "pacman -Qq >/tmp/packages-after; ! grep -E '^(nacre($|-)|orient$)' /tmp/packages-after"
            )
            finish("leftovers")
            after = inventory("after")
            changed, bad = leftovers(before, after, managed)
            (out / "inventory-diff.json").write_text(json.dumps(changed, indent=2))
            if bad:
                raise RuntimeError("Nacre leftovers: " + ", ".join(bad))
            finish()
        except (OSError, RuntimeError, ValueError, subprocess.SubprocessError) as error:
            rows.append((stage, "FAIL", time.monotonic() - started))
            detail = getattr(error.__cause__ or error, "stdout", "") or ""
            (out / "failure.txt").write_text(f"{error}\n{detail}")
        finally:
            if vm is not None and vm.poll() is None:
                vm.terminate()  # Only the exact process this invocation started.
                try:
                    vm.wait(timeout=15)
                except subprocess.TimeoutExpired:
                    vm.kill()
                    vm.wait()
            table = "Stage                 Result  Seconds\n" + "\n".join(
                f"{name:21} {result:6} {seconds:7.1f}" for name, result, seconds in rows
            )
            print(table)
            (out / "stages.txt").write_text(table + "\n")
    return int(any(result == "FAIL" for _, result, _ in rows))


if __name__ == "__main__":
    raise SystemExit(main())
