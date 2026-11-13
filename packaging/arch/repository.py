#!/usr/bin/env python3
"""Verify a tested package cut, then build a signed pacman repository."""

import argparse
from functools import cmp_to_key
import hashlib
import html
import importlib.util
import json
from pathlib import Path
import re
import shutil
import shlex
import subprocess
import time
from urllib.error import HTTPError
from urllib.request import Request, urlopen

ROOT = Path(__file__).resolve().parents[2]
SERVER = "https://nacre-desktop.github.io/nacre/arch/dev/x86_64/"


def run(*command):
    return subprocess.check_output(command, text=True, stderr=subprocess.STDOUT)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def package_info(path):
    values = {}
    for line in run("bsdtar", "-xOf", str(path), ".PKGINFO").splitlines():
        key, separator, value = line.partition(" = ")
        if separator:
            values.setdefault(key, []).append(value)
    for field in ("pkgname", "pkgver", "arch"):
        if len(values.get(field, [])) != 1:
            raise ValueError("Invalid package metadata: " + str(path))
    return values


def verify_artifacts(root, artifacts):
    sessions = json.loads((root / "packaging/sessions.json").read_text())
    expected = {
        "nacre",
        "nacre-ai",
        "nacre-brain",
        "nacre-keyring",
        "orient",
        "nacre-niri",
    } | {definition["package"] for definition in sessions.values()}
    lock = json.loads((root / "dependencies.lock.json").read_text())["orient"]
    recipe = artifacts / "nacre/PKGBUILD"
    source = re.search(
        r"nacre::git\+https://github.com/nacre-desktop/nacre.git#commit=([0-9a-f]{40})",
        recipe.read_text(),
    )
    if source is None:
        raise ValueError("Core recipe must pin its exact published source")
    spec = importlib.util.spec_from_file_location(
        "fork_recipe", root / "packaging/arch/niri/recipe.py"
    )
    fork = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(fork)
    fork_recipe = artifacts / "niri/PKGBUILD"
    if not fork_recipe.read_text().startswith(f"_commit={fork.SOURCE}\n"):
        raise ValueError("Fork recipe must pin the reviewed source")
    paths = {}
    records = {}
    for directory in ("nacre", "orient", "niri"):
        for path in sorted((artifacts / directory).glob("*.pkg.tar.zst")):
            if path.is_symlink():
                raise ValueError("Package archive must be a regular file")
            info = package_info(path)
            name, version, arch = (
                info[field][0] for field in ("pkgname", "pkgver", "arch")
            )
            if name.endswith("-debug") or name == "python-orient-colors":
                continue
            if name not in expected or name in records or arch not in ("any", "x86_64"):
                raise ValueError("Unexpected or duplicate package: " + name)
            if path.name != f"{name}-{version}-{arch}.pkg.tar.zst":
                raise ValueError("Archive filename and metadata disagree")
            if name != "orient":
                build = run("bsdtar", "-xOf", str(path), ".BUILDINFO")
                tested_recipe = fork_recipe if name == "nacre-niri" else recipe
                if (
                    "pkgbuild_sha256sum = " + digest(tested_recipe)
                    not in build.splitlines()
                ):
                    raise ValueError(
                        "Archive does not match the tested recipe: " + name
                    )
            paths[name] = path
            records[name] = dict(
                filename=path.name,
                version=version,
                sha256=digest(path),
                dependencies=info.get("depend", []),
            )
    if records.keys() != expected:
        raise ValueError(
            "Missing package families: " + ", ".join(sorted(expected - records.keys()))
        )
    core = records["nacre"]
    required_orient = "orient=" + lock["version"].replace("-dev.", "dev")
    if required_orient not in core["dependencies"]:
        raise ValueError("Core must depend on the separately tested Orient version")
    if (
        records["orient"]["version"].rsplit("-", 1)[0]
        != required_orient.split("=", 1)[1]
    ):
        raise ValueError("Orient package and Core engine lock disagree")
    # Every Core update must also carry the current public key (key renewal).
    if (
        "nacre-keyring" not in core["dependencies"]
        or records["nacre-keyring"]["version"] != core["version"]
    ):
        raise ValueError("Core must depend on the nacre-keyring of its own cut")
    for name in expected - {"nacre", "nacre-keyring", "orient", "nacre-niri"}:
        if (
            "nacre=" + core["version"].rsplit("-", 1)[0]
            not in records[name]["dependencies"]
        ):
            raise ValueError("Split package and Core version disagree: " + name)
    proof = artifacts / "installed-runtime.log"
    if (
        not proof.is_file()
        or proof.read_text().splitlines().count("FRESH_USER_PACKAGE_PROOF_OK") != 4
    ):
        raise ValueError(
            "Require passing installed-runtime proof for all four session selections"
        )
    return paths, records, source.group(1)


def sign(path, fingerprint):
    run(
        "gpg",
        "--batch",
        "--local-user",
        fingerprint,
        "--detach-sign",
        "--output",
        str(path) + ".sig",
        str(path),
    )
    run("gpg", "--batch", "--verify", str(path) + ".sig", str(path))


def check_signing_expiry(root, fingerprint, days=90):
    """Refuse to sign with a subkey whose published expiry is near."""
    probe = subprocess.run(
        ["gpg", "--batch", "--status-fd=2", "--local-user", fingerprint, "-b"],
        input=b"",
        capture_output=True,
        check=True,
    )
    signer = next(
        line.split()[7]
        for line in probe.stderr.decode().splitlines()
        if line.startswith("[GNUPG:] SIG_CREATED ")
    )
    # Users verify with the shipped public key, so its expiry is what counts.
    listing = run(
        "gpg",
        "--batch",
        "--with-colons",
        "--with-subkey-fingerprint",
        "--show-keys",
        str(root / "packaging/keys/nacre-repo.asc"),
    )
    rows = [line.split(":") for line in listing.splitlines()]
    expiry = next(
        (
            key[6]
            for key, fpr in zip(rows, rows[1:], strict=False)
            if key[0] in ("pub", "sub") and fpr[0] == "fpr" and fpr[9] == signer
        ),
        None,
    )
    if expiry is None:
        raise ValueError(
            f"Signing key {signer} is not in packaging/keys/nacre-repo.asc"
        )
    # An expired primary key also invalidates its subkeys.
    primary = next(row[6] for row in rows if row[0] == "pub")
    expiry = min((int(value) for value in (expiry, primary) if value), default=None)
    if expiry is not None and expiry < time.time() + days * 86400:
        raise ValueError(
            f"Signing key {signer} is valid only until "
            + time.strftime("%Y-%m-%d", time.gmtime(expiry))
            + f", within {days} days. Extend it and publish the renewed"
            " packaging/keys/nacre-repo.asc first (docs/specs/0412-keyring-package.md)."
        )


def fetch_previous(destination):
    """Retain previous signed package URLs across an atomic Pages deployment."""
    destination.mkdir(parents=True, exist_ok=True)

    def download(name, limit):
        request = Request(SERVER + name, headers={"Cache-Control": "no-cache"})
        with (
            urlopen(request, timeout=60) as response,
            (destination / name).open("wb") as stream,
        ):
            size = 0
            while block := response.read(1024 * 1024):
                size += len(block)
                if size > limit:
                    raise ValueError("Repository download exceeds its size limit")
                stream.write(block)

    try:
        download("release.json", 1024 * 1024)
    except HTTPError as error:
        if error.code == 404:
            return False  # First publication only; other network failures are fatal.
        raise
    download("release.json.sig", 65536)
    run(
        "gpg",
        "--batch",
        "--verify",
        str(destination / "release.json.sig"),
        str(destination / "release.json"),
    )
    previous = json.loads((destination / "release.json").read_text())
    fingerprint = (ROOT / "packaging/keys/nacre-repo.fingerprint").read_text().strip()
    if previous["schema"] != 1 or previous["fingerprint"] != fingerprint:
        raise ValueError("Repository signing identity changed")
    names = {
        "nacre",
        "nacre-ai",
        "nacre-brain",
        "nacre-keyring",
        "nacre-niri",
        "orient",
    } | {
        item["package"]
        for item in json.loads((ROOT / "packaging/sessions.json").read_text()).values()
    }
    for name, records in previous["retained"].items():
        if name not in names or len(records) > 3:
            raise ValueError("Unexpected retained package family")
        for record in records:
            filename = record["filename"]
            if not re.fullmatch(
                re.escape(name) + r"-[a-zA-Z0-9_.+:-]+-(?:any|x86_64)\.pkg\.tar\.zst",
                filename,
            ):
                raise ValueError("Invalid retained archive name")
            download(filename, 256 * 1024 * 1024)
            download(filename + ".sig", 65536)
            if digest(destination / filename) != record["sha256"]:
                raise ValueError("Retained package checksum changed")
            run(
                "gpg",
                "--batch",
                "--verify",
                str(destination / filename) + ".sig",
                str(destination / filename),
            )
    return True


def render_bootstrap(root):
    bootstrap = (root / "packaging/arch/bootstrap.sh").read_text()
    if "@NACRE_PACMAN_FRONTEND@" in bootstrap:
        frontend = (root / "tools/package_transaction.py").read_text() + "\nmain()\n"
        bootstrap = bootstrap.replace("@NACRE_PACMAN_FRONTEND@", shlex.quote(frontend))
    return bootstrap


def build(root, artifacts, output, fingerprint, revision, run_id, previous=None):
    if (
        not re.fullmatch(r"[0-9A-F]{40}", fingerprint)
        or not re.fullmatch(r"[0-9a-f]{40}", revision)
        or not run_id.isdecimal()
    ):
        raise ValueError("Invalid publication identity")
    paths, records, source = verify_artifacts(root, artifacts)
    if source != revision:
        raise ValueError("Publication must identify the exact packaged source")
    destination = output / "arch/dev/x86_64"
    destination.mkdir(parents=True)
    retained = {name: [record.copy()] for name, record in records.items()}
    if previous is not None:
        prior = previous / "release.json"
        run("gpg", "--batch", "--verify", str(prior) + ".sig", str(prior))
        old = json.loads(prior.read_text())
        if old["schema"] != 1 or old["fingerprint"] != fingerprint:
            raise ValueError("Previous repository signing identity changed")
        for name, current in records.items():
            old_current = old["packages"].get(name)
            if (
                old_current is not None
                and int(run("vercmp", current["version"], old_current["version"])) < 0
            ):
                raise ValueError("Refusing to publish an older package cut")
            older = old.get("retained", {}).get(name, [])
            older.sort(
                key=cmp_to_key(
                    lambda a, b: -int(run("vercmp", a["version"], b["version"]))
                )
            )
            for record in older:
                if record["version"] == current["version"]:
                    if record["sha256"] != current["sha256"]:
                        raise ValueError(
                            "An immutable package version was rebuilt: " + name
                        )
                    continue
                if len(retained[name]) >= 3:
                    break
                filename = record["filename"]
                if not re.fullmatch(
                    re.escape(name)
                    + r"-[a-zA-Z0-9_.+:-]+-(?:any|x86_64)\.pkg\.tar\.zst",
                    filename,
                ):
                    raise ValueError("Invalid retained archive name")
                archive = previous / filename
                if digest(archive) != record["sha256"]:
                    raise ValueError("Retained package checksum changed")
                run("gpg", "--batch", "--verify", str(archive) + ".sig", str(archive))
                shutil.copy2(archive, destination / filename)
                shutil.copy2(
                    str(archive) + ".sig", str(destination / filename) + ".sig"
                )
                retained[name].append(record)
    for name, path in paths.items():
        copied = destination / records[name]["filename"]
        shutil.copy2(path, copied)
        sign(copied, fingerprint)
    database = destination / "nacre-dev.db.tar.gz"
    run(
        "repo-add",
        "--sign",
        "--key",
        fingerprint,
        str(database),
        *(str(destination / record["filename"]) for record in records.values()),
    )
    # Pages artifacts cannot contain symlinks, including repo-add's DB aliases.
    for path in destination.iterdir():
        if path.is_symlink():
            content = path.read_bytes()
            path.unlink()
            path.write_bytes(content)
    release = dict(
        schema=1,
        channel="dev",
        fingerprint=fingerprint,
        testedRevision=revision,
        sourceRevision=source,
        runId=run_id,
        packages=records,
        retained=retained,
    )
    shutil.copy2(artifacts / "nacre/PKGBUILD", destination / "PKGBUILD")
    # Pages does not serve dot-prefixed paths. Keep recipe metadata downloadable.
    shutil.copy2(artifacts / "nacre/.SRCINFO", destination / "SRCINFO")
    manifest = destination / "release.json"
    manifest.write_text(json.dumps(release, indent=2, sort_keys=True) + "\n")
    sign(manifest, fingerprint)
    package_checksums = destination / "PACKAGE-SHA256SUMS"
    assets = [destination / item["filename"] for item in records.values()]
    assets += [destination / "PKGBUILD", destination / "SRCINFO", manifest]
    package_checksums.write_text(
        "".join(digest(path) + "  " + path.name + "\n" for path in assets)
    )
    sign(package_checksums, fingerprint)
    checksums = destination / "SHA256SUMS"
    checksums.write_text(
        "".join(
            digest(path) + "  " + path.name + "\n"
            for path in sorted(destination.iterdir())
            if path.is_file()
        )
    )
    sign(checksums, fingerprint)
    shutil.copy2(root / "packaging/keys/nacre-repo.asc", output / "nacre-repo.asc")
    (output / "install-arch.sh").write_text(render_bootstrap(root))
    sign(output / "install-arch.sh", fingerprint)
    links = "\n".join(
        '<li><a href="arch/dev/x86_64/'
        + html.escape(record["filename"], quote=True)
        + '">'
        + html.escape(name + " " + record["version"])
        + "</a></li>"
        for name, record in sorted(records.items())
    )
    title = (
        "Nacre development packages"
        if "dev" in records["nacre"]["version"]
        else "Nacre packages"
    )
    (output / "index.html").write_text(
        '<!doctype html><meta charset="utf-8"><title>'
        + title
        + "</title><h1>"
        + title
        + '</h1><p>Tested Arch/CachyOS preview packages; x86_64 only. Orient is a separate dependency. Save and review <a href="install-arch.sh">the Arch installer</a> before running it. Updates use pacman -Syu.</p><p>Signing fingerprint: <code>'
        + fingerprint
        + "</code></p><ul>"
        + links
        + '</ul><p><a href="arch/dev/x86_64/release.json">Source receipt</a> · <a href="https://github.com/nacre-desktop/nacre/actions/runs/'
        + run_id
        + '">Build proof</a></p>\n'
    )
    return release


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("artifacts", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--revision")
    parser.add_argument("--run-id")
    parser.add_argument("--previous", type=Path)
    parser.add_argument("--fetch-previous", action="store_true")
    args = parser.parse_args()
    if args.fetch_previous:
        fetch_previous(args.artifacts)
    elif args.output is None:
        _, records, source = verify_artifacts(ROOT, args.artifacts)
        print(json.dumps(dict(sourceRevision=source, packages=records)))
    else:
        fingerprint = (
            (ROOT / "packaging/keys/nacre-repo.fingerprint").read_text().strip()
        )
        check_signing_expiry(ROOT, fingerprint)
        build(
            ROOT,
            args.artifacts,
            args.output,
            fingerprint,
            args.revision or "",
            args.run_id or "",
            args.previous,
        )


if __name__ == "__main__":
    main()
