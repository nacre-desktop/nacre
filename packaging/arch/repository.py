#!/usr/bin/env python3
"""Verify a tested package cut, then build a signed pacman repository."""

import argparse
from functools import cmp_to_key
import hashlib
import html
import json
from pathlib import Path
import re
import shutil
import subprocess
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
    expected = {"nacre", "nacre-ai", "nacre-brain", "orient"} | {
        definition["package"] for definition in sessions.values()
    }
    lock = json.loads((root / "dependencies.lock.json").read_text())["orient"]
    recipe = artifacts / "nacre/PKGBUILD"
    source = re.search(
        r"nacre::git\+https://github.com/nacre-desktop/nacre.git#commit=([0-9a-f]{40})",
        recipe.read_text(),
    )
    if source is None:
        raise ValueError("Core recipe must pin its exact published source")
    paths = {}
    records = {}
    for directory in ("nacre", "orient"):
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
                if "pkgbuild_sha256sum = " + digest(recipe) not in build.splitlines():
                    raise ValueError(
                        "Archive does not match the tested Core recipe: " + name
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
    for name in expected - {"nacre", "orient"}:
        if (
            "nacre=" + core["version"].rsplit("-", 1)[0]
            not in records[name]["dependencies"]
        ):
            raise ValueError("Split package and Core version disagree: " + name)
    proof = artifacts / "installed-runtime.log"
    if (
        not proof.is_file()
        or proof.read_text().splitlines().count("FRESH_USER_PACKAGE_PROOF_OK") != 3
    ):
        raise ValueError(
            "Require passing installed-runtime proof for all three session selections"
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
    names = {"nacre", "nacre-ai", "nacre-brain", "orient"} | {
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
    manifest = destination / "release.json"
    manifest.write_text(json.dumps(release, indent=2, sort_keys=True) + "\n")
    sign(manifest, fingerprint)
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
    shutil.copy2(root / "packaging/arch/bootstrap.sh", output / "install-arch.sh")
    links = "\n".join(
        '<li><a href="arch/dev/x86_64/'
        + html.escape(record["filename"], quote=True)
        + '">'
        + html.escape(name + " " + record["version"])
        + "</a></li>"
        for name, record in sorted(records.items())
    )
    (output / "index.html").write_text(
        '<!doctype html><meta charset="utf-8"><title>Nacre development packages</title><h1>Nacre development packages</h1><p>Tested Arch/CachyOS preview packages; x86_64 only. Orient is a separate dependency. Save and review <a href="install-arch.sh">the Arch installer</a> before running it. Updates use pacman -Syu.</p><p>Signing fingerprint: <code>'
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
