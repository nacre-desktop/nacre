"""Read-only normalization shared by the frozen Colors producer and native replay.

Only disposable roots, stat identities and their derived cache names are replaced.
Current JSON spacing/newlines are checked before canonicalizing volatile fields.
"""

import base64
import hashlib
import json
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

    def snapshot(self):
        result = {}
        for path in sorted(self.home.rglob("*")):
            name = self.normalize(str(path.relative_to(self.home)))
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
