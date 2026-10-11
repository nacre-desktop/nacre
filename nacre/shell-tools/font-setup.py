#!/usr/bin/env python3
"""Provision pinned upstream fonts with verified ownership and rollback."""

import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import urllib.request
import uuid

MANIFEST = Path(__file__).with_name("font-assets.json")


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest() if path.is_file() else None


def read(path):
    return json.loads(path.read_text()) if path.exists() else {}


def write(path, data):
    path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    fd, temporary = tempfile.mkstemp(dir=path.parent)
    try:
        with os.fdopen(fd, "wb") as stream:
            stream.write(data)
        os.replace(temporary, path)
    finally:
        Path(temporary).unlink(missing_ok=True)


def roots(home):
    return (
        home / ".local/share/fonts/nacre",
        home / ".local/state/nacre/fonts/installed.json",
    )


def entries(home, manifest):
    canonical, registry = roots(home)
    for directory in (canonical,):
        if directory.is_symlink():
            raise ValueError(
                "Font directory is a symlink; reconcile it before provisioning"
            )
    known = read(registry)
    selected = []
    for group in manifest["groups"]:
        if not (
            group["default"]
            or any(
                (root / row["name"]).exists()
                for root in (canonical,)
                for row in group["files"]
            )
            or group["id"] in known.get("groups", [])
        ):
            continue
        for row in group["files"]:
            name = row["name"]
            if Path(name).name != name or name in (".", ".."):
                raise ValueError("Invalid font asset filename")
            target = canonical / name
            if target.is_symlink():
                raise ValueError(f"Font asset is a symlink: {name}")
            current = digest(target)
            if target.exists() and not target.is_file():
                raise ValueError(f"Font asset is not a file: {name}")
            owned = known.get("files", {}).get(name)
            if current not in (None, row["sha256"], owned) or (
                current is not None and owned is None and current != row["sha256"]
            ):
                raise ValueError(f"Local font edit preserved: {name}")
            selected.append(
                dict(
                    row,
                    group=group["id"],
                    target=target,
                    current=current,
                )
            )
    return selected


def plan(home, manifest):
    selected = entries(home, manifest)
    return {
        "install": [row["name"] for row in selected if row["current"] != row["sha256"]],
        "groups": sorted({row["group"] for row in selected}),
    }


def download(row):
    with urllib.request.urlopen(row["url"], timeout=30) as response:
        raw = response.read(row["size"] + 1)
    if len(raw) != row["size"] or hashlib.sha256(raw).hexdigest() != row["sha256"]:
        raise ValueError(f"Upstream font checksum mismatch: {row['name']}")
    return raw


def cache_refresh(home):
    subprocess.run(
        ["fc-cache", "-f", str(home / ".local/share/fonts")],
        check=True,
        capture_output=True,
    )


def restore(home, transaction, refresh=True):
    state = home / ".local/state/nacre/fonts"
    state.mkdir(parents=True, exist_ok=True, mode=0o700)
    with (state / "setup.lock").open("a") as owner:
        fcntl.flock(owner, fcntl.LOCK_EX)
        return restore_locked(home, transaction, refresh=refresh)


def restore_locked(home, transaction, refresh=True):
    record = read(transaction / "transaction.json")
    # Validate all destinations and backup bytes before touching any file.
    payloads = {}
    for operation in record["operations"]:
        relative = Path(operation["path"])
        allowed = relative == Path(".local/state/nacre/fonts/installed.json") or (
            relative.parent == Path(".local/share/fonts/nacre")
        )
        if relative.is_absolute() or ".." in relative.parts or not allowed:
            raise ValueError("Invalid font rollback path")
        path = home / relative
        if path.is_symlink() or digest(path) not in (
            operation["after"],
            operation["before"],
        ):
            raise ValueError(f"Later font edit preserved: {path.name}")
        if operation["before"] is not None:
            raw = (transaction / "before" / relative).read_bytes()
            if hashlib.sha256(raw).hexdigest() != operation["before"]:
                raise ValueError("Font rollback backup checksum mismatch")
            payloads[operation["path"]] = raw
    for operation in reversed(record["operations"]):
        path = home / operation["path"]
        if operation["before"] is None:
            path.unlink(missing_ok=True)
        else:
            write(path, payloads[operation["path"]])
            path.chmod(operation["mode"])
    for directory in (roots(home)[0],):
        if directory.is_dir() and not any(directory.iterdir()):
            directory.rmdir()
    if refresh:
        cache_refresh(home)


def apply(home, manifest, fetch=download, refresh=True):
    state = home / ".local/state/nacre/fonts"
    state.mkdir(parents=True, exist_ok=True, mode=0o700)
    with (state / "setup.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        selected = entries(home, manifest)
        expected = {row["name"]: row["sha256"] for row in selected}
        registry = roots(home)[1]
        registry_data = json.dumps(
            {
                "version": 1,
                "files": expected,
                "groups": sorted({row["group"] for row in selected}),
            },
            sort_keys=True,
        ).encode()
        if (
            not any(row["current"] != row["sha256"] for row in selected)
            and registry.exists()
            and registry.read_bytes() == registry_data
        ):
            return {"changed": False, "transaction": None}
        # Validate all input bytes before promoting or removing a single asset.
        payloads = {}
        for row in selected:
            if row["current"] == row["sha256"]:
                raw = row["target"].read_bytes()
            else:
                raw = fetch(row)
            if (
                len(raw) != row["size"]
                or hashlib.sha256(raw).hexdigest() != row["sha256"]
            ):
                raise ValueError(f"Font asset checksum mismatch: {row['name']}")
            payloads[row["name"]] = raw
        for row in selected:
            if digest(row["target"]) != row["current"]:
                raise ValueError(f"Font changed during staging: {row['name']}")
        transaction = state / "backups" / uuid.uuid4().hex
        transaction.mkdir(parents=True, mode=0o700)
        operations = []

        def change(path, raw, mode):
            relative = str(path.relative_to(home))
            before = digest(path)
            if before is not None:
                backup = transaction / "before" / relative
                backup.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(path, backup)
            operation = {
                "path": relative,
                "before": before,
                "after": hashlib.sha256(raw).hexdigest() if raw is not None else None,
                "mode": path.stat().st_mode & 0o777 if before else mode,
            }
            operations.append(operation)
            write(
                transaction / "transaction.json",
                json.dumps({"operations": operations}).encode(),
            )
            if raw is None:
                path.unlink()
            else:
                write(path, raw)
                path.chmod(mode)

        try:
            for row in selected:
                if row["current"] != row["sha256"]:
                    change(row["target"], payloads[row["name"]], 0o644)
            change(registry, registry_data, 0o600)
            if refresh:
                cache_refresh(home)
        except Exception:  # noqa: BLE001 — Rollback must run for any failure before the original exception is re-raised.
            if operations:
                restore_locked(home, transaction, refresh=refresh)
            raise
        return {"changed": True, "transaction": transaction.name}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--apply", action="store_true")
    parser.add_argument("--rollback")
    args = parser.parse_args()
    home = Path.home()
    if args.rollback:
        if not args.rollback.isalnum():
            raise ValueError("Invalid font transaction ID")
        restore(home, home / ".local/state/nacre/fonts/backups" / args.rollback)
        return
    manifest = read(MANIFEST)
    print(json.dumps(apply(home, manifest) if args.apply else plan(home, manifest)))


if __name__ == "__main__":
    main()
