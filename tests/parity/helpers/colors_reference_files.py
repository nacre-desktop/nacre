"""Read-only normalization shared by the frozen Colors producer and native replay.

Only disposable roots, stat identities and their derived cache names are replaced.
Current JSON spacing/newlines are checked before canonicalizing volatile fields.
"""

import base64
import hashlib
import json
import os
from pathlib import Path
import stat


def signature(path):
    path = Path(path).resolve(strict=True)
    value = path.stat()
    return [
        str(path),
        value.st_dev,
        value.st_ino,
        value.st_size,
        value.st_mtime_ns,
        value.st_ctime_ns,
    ]


def unpack(value, strings):
    if isinstance(value, dict):
        if set(value) == {"$string"}:
            return strings[value["$string"]]
        return {key: unpack(child, strings) for key, child in value.items()}
    if isinstance(value, list):
        return [unpack(child, strings) for child in value]
    return value


class Files:
    def __init__(self, home, signatures):
        self.home = Path(home)
        self.signatures = signatures
        self.aliases = {}

    def normalize(self, value):
        if isinstance(value, str):
            value = value.replace(str(self.home), "/fixture/home")
            for old, new in sorted(
                self.aliases.items(), key=lambda pair: -len(pair[0])
            ):
                value = value.replace(old, new)
            return value
        if isinstance(value, list):
            return [self.normalize(child) for child in value]
        if isinstance(value, dict):
            return {
                self.normalize(key): self.stat_value(child)
                if key == "signature"
                else self.normalize(child)
                for key, child in value.items()
            }
        return value

    def stat_value(self, value):
        if not isinstance(value, list) or len(value) != 6:
            return self.normalize(value)
        if value not in self.signatures:
            raise ValueError(
                "Cache signature does not match an observed filesystem input"
            )
        return [
            self.normalize(value[0]),
            "@DEV@",
            "@INODE@",
            value[3],
            value[4],
            "@CTIME_NS@",
        ]

    def discover(self, results):
        records = []
        folder = self.home / ".cache/nacre/colors"
        for path in sorted(
            folder.glob("*.json"), key=lambda path: path.stat().st_mtime_ns
        ):
            record = json.loads(path.read_text())
            identity = path.stem
            fixed = [
                self.stat_value(record["signature"]),
                self.normalize(record["request"]),
                record["palette"]["source"].get("digest"),
            ]
            label = (
                "@PREVIEW_"
                + hashlib.sha256(
                    json.dumps(fixed, sort_keys=True).encode()
                ).hexdigest()[:16]
                + "@"
            )
            self.aliases[identity] = label
            self.aliases[identity[:16]] = label + "-short16"
            self.aliases[identity[:8]] = label + "-short8"
            records.append((identity, record))

        def media(value):
            if isinstance(value, dict):
                if all(
                    key in value for key in ("path", "poster", "thumbnail", "preview")
                ):
                    poster = value["poster"].replace(str(self.home), "/fixture/home")
                    label = (
                        "@MEDIA_"
                        + hashlib.sha256(poster.encode()).hexdigest()[:16]
                        + "@"
                    )
                    for name in ("thumbnail", "preview"):
                        stem = Path(value[name]).stem
                        if stem.endswith("-" + name):
                            self.aliases[stem.removesuffix("-" + name)] = label
                for child in value.values():
                    media(child)
            elif isinstance(value, list):
                for child in value:
                    media(child)

        media(results)
        media([record for _, record in records])
        for path in (self.home / ".cache/orient/palettes").glob("rust-*.json"):
            value = json.loads(path.read_text())
            settings = self.normalize(value["input"])
            identity = hashlib.sha256(
                json.dumps(settings, sort_keys=True).encode()
            ).hexdigest()[:24]
            self.aliases[path.stem] = "@PALETTE_" + identity + "@"
        for path in self.home.rglob("*.png"):
            if (
                path.parent.name in ("branding", "login-images")
                and len(path.stem) == 64
            ):
                self.aliases[path.stem] = (
                    "@PNG_" + hashlib.sha256(path.read_bytes()).hexdigest() + "@"
                )

    def validate_native_engine_identity_cache(self, path):
        """Validate the one expected runtime cache before omitting it from parity."""
        metadata = path.lstat()
        if (
            not stat.S_ISREG(metadata.st_mode)
            or metadata.st_uid != os.getuid()
            or metadata.st_mode & 0o022
            or metadata.st_size > 4096
        ):
            raise ValueError("Unsafe native engine identity cache")
        raw = path.read_bytes()
        if len(raw) > 4096:
            raise ValueError("Oversized native engine identity cache")
        record = json.loads(raw)
        if json.dumps(record, separators=(",", ":")) != raw.decode():
            raise ValueError("Native engine identity cache serialization changed")
        if not isinstance(record, dict) or set(record) != {
            "engine",
            "identity",
            "sha256",
        }:
            raise ValueError("Invalid native engine identity cache fields")
        pin_path = (
            Path(__file__).resolve().parents[3]
            / "nacre/shell-cli/src/nacre_shell/engine-pin.json"
        )
        pin = json.loads(pin_path.read_text())
        if record["engine"] != pin.get("engine"):
            raise ValueError("Native engine identity cache has the wrong engine ID")
        identity = record["identity"]
        fields = {
            "device",
            "inode",
            "size",
            "mtime_sec",
            "mtime_nsec",
            "ctime_sec",
            "ctime_nsec",
        }
        if not isinstance(identity, dict) or set(identity) != fields:
            raise ValueError("Invalid native executable identity fields")
        for key in ("device", "inode"):
            value = identity[key]
            if (
                not isinstance(value, str)
                or not value.isascii()
                or not value.isdecimal()
                or len(value) > 20
                or (len(value) > 1 and value.startswith("0"))
                or int(value) > 2**64 - 1
            ):
                raise ValueError("Invalid native executable device/inode")
        safe_integer = 2**53 - 1
        if (
            type(identity["size"]) is not int
            or not 0 <= identity["size"] <= safe_integer
            or any(
                type(identity[key]) is not int
                or not -safe_integer <= identity[key] <= safe_integer
                for key in ("mtime_sec", "ctime_sec")
            )
            or any(
                type(identity[key]) is not int or not 0 <= identity[key] <= 999_999_999
                for key in ("mtime_nsec", "ctime_nsec")
            )
        ):
            raise ValueError("Invalid native executable timestamps/size")
        digest = record["sha256"]
        if (
            not isinstance(digest, str)
            or len(digest) != 64
            or any(character not in "0123456789abcdef" for character in digest)
        ):
            raise ValueError("Invalid native engine identity SHA-256")

    def snapshot(self):
        result = {}
        for path in sorted(self.home.rglob("*")):
            name = self.normalize(str(path.relative_to(self.home)))
            if path == self.home / ".cache/nacre/native-engine-identity.json":
                self.validate_native_engine_identity_cache(path)
                continue
            if path.is_symlink():
                result[name] = dict(link=self.normalize(str(path.readlink())))
            elif path.is_file():
                raw = path.read_bytes()
                try:
                    text = raw.decode()
                except UnicodeDecodeError:
                    content = dict(base64=base64.b64encode(raw).decode())
                else:
                    if (
                        path.parent == self.home / ".cache/nacre/colors"
                        and path.suffix == ".json"
                    ):
                        value = json.loads(text)
                        if json.dumps(value) != text:
                            raise ValueError("Colors record JSON serialization changed")
                        text = json.dumps(self.normalize(value))
                    else:
                        text = self.normalize(text)
                    content = dict(text=text)
                result[name] = dict(mode=stat.S_IMODE(path.stat().st_mode), **content)
        return result
