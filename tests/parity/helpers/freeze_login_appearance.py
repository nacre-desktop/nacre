#!/usr/bin/env python3
"""Freeze current login PNGs, cache stamps and publication boundaries once."""

import base64
from contextlib import redirect_stderr, redirect_stdout
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import sys
import tempfile
from unittest.mock import patch
from PIL import Image, ImageCms

ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / "nacre/shell-tools/login-appearance.py"
SOURCE_SHA = "20f04953a1324fa26ba640989b24aced6b4a8cd4914d687d0ec34375e066bd07"
DESTINATION = (
    ROOT / "crates/nacre-helper/tests/fixtures/login-appearance-reference.json"
)
STAMP_NS = 1700000000123456789


def picture(kind):
    width, height = (
        (2801, 1803)
        if kind == "large-landscape"
        else (1703, 2701)
        if kind == "large-portrait"
        else (37, 23)
    )
    image = Image.new("RGB", (width, height))
    image.putdata(
        [
            ((x * 17 + y * 3) % 256, (x * 5 + y * 29) % 256, (x ^ y) % 256)
            for y in range(height)
            for x in range(width)
        ]
    )
    kwargs = {}
    format = "PNG"
    if kind == "rgba":
        image = image.convert("RGBA")
        image.putalpha(Image.linear_gradient("L").resize(image.size))
    elif kind == "palette":
        image = image.quantize(colors=16)
        kwargs["transparency"] = 0
    elif kind == "gray16":
        image = Image.new("I;16", (37, 23))
        image.putdata(
            [(x * 61 + y * 257) % 65536 for y in range(23) for x in range(37)]
        )
    elif kind == "icc":
        kwargs["icc_profile"] = ImageCms.ImageCmsProfile(
            ImageCms.createProfile("sRGB")
        ).tobytes()
    elif kind == "transparent-rgb":
        kwargs["transparency"] = (17, 5, 1)
    elif kind == "jpeg-rotated":
        format = "JPEG"
        exif = Image.Exif()
        exif[274] = 6
        exif[270] = "Public fixture"
        kwargs.update(exif=exif, quality=85)
    elif kind == "webp":
        format = "WEBP"
        kwargs["lossless"] = True
    elif kind == "gif":
        format = "GIF"
    output = io.BytesIO()
    image.save(output, format=format, **kwargs)
    return output.getvalue()


def capture():
    if DESTINATION.exists():
        raise ValueError("Refusing to overwrite login appearance reference")
    if hashlib.sha256(SOURCE.read_bytes()).hexdigest() != SOURCE_SHA:
        raise ValueError("Login appearance reference source changed")
    sys.path.insert(0, str(SOURCE.parent))
    spec = importlib.util.spec_from_file_location("login_reference", SOURCE)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    inputs = {
        kind: picture(kind)
        for kind in (
            "rgb",
            "rgba",
            "palette",
            "gray16",
            "icc",
            "transparent-rgb",
            "jpeg-rotated",
            "webp",
            "gif",
            "large-landscape",
            "large-portrait",
        )
    }
    cases = []
    for kind, data in inputs.items():
        with tempfile.TemporaryDirectory(prefix="nacre-login-reference-") as folder:
            base = Path(folder)
            source = base / "home/images/input.image"
            source.parent.mkdir(parents=True)
            source.write_bytes(data)
            os.utime(source, ns=(STAMP_NS, STAMP_NS))
            cache = base / "home/cache"
            target, stamp = module.prepare(source, cache)
            before = target.stat().st_mtime_ns
            with patch.object(
                module.Image,
                "open",
                side_effect=AssertionError("Cache reuse must not decode"),
            ):
                cached, repeated = module.prepare(source, cache)
            output = target.read_bytes()
            normalized_stamp = stamp.replace(str(base), "/fixture")
            cases.append(
                {
                    "operation": "prepare",
                    "name": kind,
                    "input": kind,
                    "mtime_ns": STAMP_NS,
                    "stamp": normalized_stamp,
                    "target": "home/cache/"
                    + hashlib.sha256(normalized_stamp.encode()).hexdigest()
                    + ".png",
                    "png": base64.b64encode(output).decode(),
                    "pixel_sha256": hashlib.sha256(
                        Image.open(io.BytesIO(output)).tobytes()
                    ).hexdigest(),
                    "size": list(Image.open(io.BytesIO(output)).size),
                    "mode": target.stat().st_mode & 0o777,
                    "cache_mode": cache.stat().st_mode & 0o777,
                    "reused": cached == target
                    and repeated == stamp
                    and cached.stat().st_mtime_ns == before,
                }
            )
    colors = {role: "aabbcc" for role in module.ROLES.values()}
    for name, options in [
        ("canonical-private", {}),
        ("custom-public", {"public_exists": True}),
        ("staged-home", {"staged": True, "public_exists": True}),
        ("staged-preview", {"custom_preview": True, "public_exists": True}),
        ("system-public-override", {"system": True, "public_exists": True}),
        ("invalid-color", {"invalid": True, "public_exists": True}),
        ("missing-role", {"missing": True}),
    ]:
        with tempfile.TemporaryDirectory(prefix="nacre-login-publish-") as folder:
            base = Path(folder)
            home = base / "home"
            home.mkdir()
            requested = base / "stage" if options.get("staged") else home
            source = base / "input.image"
            source.write_bytes(inputs["rgb"])
            os.utime(source, ns=(STAMP_NS, STAMP_NS))
            public = base / "public"
            if options.get("public_exists"):
                public.mkdir()
                (public / "background.png").write_bytes(b"original image")
            preview = base / "custom-preview" if options.get("custom_preview") else None
            palette = dict(colors)
            if options.get("invalid"):
                palette["primary"] = "bad\n[Autologin]"
            if options.get("missing"):
                del palette["surface"]
            out, err = io.StringIO(), io.StringIO()
            error = None
            result = None
            with (
                patch.object(module, "HOME", home),
                patch.object(
                    module,
                    "PUBLIC",
                    public
                    if options.get("system")
                    else Path("/var/lib/nacre/login/appearance"),
                ),
                redirect_stdout(out),
                redirect_stderr(err),
            ):
                try:
                    result = module.publish(
                        palette, source, preview, public, home=requested
                    )
                except (OSError, ValueError, KeyError) as failure:
                    error = {"type": type(failure).__name__, "message": str(failure)}
            replacements = {str(base): "/fixture"}
            for path in base.rglob("image-stamp.txt"):
                original = path.read_text()
                normalized = original.replace(str(base), "/fixture")
                replacements[hashlib.sha256(original.encode()).hexdigest()] = (
                    hashlib.sha256(normalized.encode()).hexdigest()
                )

            def normalize(text, replacements=replacements):
                for old, new in replacements.items():
                    text = text.replace(old, new)
                return text

            files = {}
            for path in sorted(base.rglob("*")):
                if path.is_file() and path != source:
                    content = path.read_bytes()
                    try:
                        content = normalize(content.decode()).encode()
                    except UnicodeDecodeError:
                        pass
                    files[normalize(str(path.relative_to(base)))] = {
                        "bytes": base64.b64encode(content).decode(),
                        "mode": path.stat().st_mode & 0o777,
                    }
            cases.append(
                {
                    "operation": "publish",
                    "name": name,
                    "options": options,
                    "input": "rgb",
                    "colors": palette,
                    "result": normalize(str(result)) if result else None,
                    "error": error,
                    "stdout": out.getvalue(),
                    "stderr": err.getvalue(),
                    "files": files,
                }
            )
    DESTINATION.write_text(
        json.dumps(
            {
                "source_sha256": SOURCE_SHA,
                "normalizations": "Disposable root becomes /fixture; only its derived SHA256 cache basename is recomputed. PNG bytes and all other fields remain exact.",
                "inputs": {
                    name: base64.b64encode(data).decode()
                    for name, data in inputs.items()
                },
                "cases": cases,
            },
            indent=2,
        )
        + "\n"
    )
    print(
        f"Frozen {len(cases)} login cases with {len(inputs)} exact encoded input vectors"
    )


if __name__ == "__main__":
    capture()
