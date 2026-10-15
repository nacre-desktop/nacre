#!/usr/bin/env python3
"""Replay every immutable lock preparation file through native code."""

import base64
import gzip
import json
import os
from pathlib import Path
import subprocess
import tempfile

from colors_reference_files import Files
from lock_font_fixture import setup, fingerprints

ROOT = Path(__file__).resolve().parents[3]


def main():
    reference = json.loads(
        gzip.decompress(
            (
                ROOT
                / "crates/nacre-helper/tests/fixtures/lock-prepare-isolated-reference.json.gz"
            ).read_bytes()
        )
    )
    for case in reference["cases"]:
        with tempfile.TemporaryDirectory(prefix="nacre-lock-prepare-native-") as folder:
            base = Path(folder)
            home = base / "home"
            home.mkdir()
            for name, entry in case["inputs"].items():
                path = home / name
                path.parent.mkdir(parents=True, exist_ok=True)
                if "directory" in entry:
                    path.mkdir(exist_ok=True)
                elif "link" in entry:
                    path.symlink_to(entry["link"].replace("/fixture/home", str(home)))
                else:
                    path.write_bytes(
                        entry["text"].replace("/fixture/home", str(home)).encode()
                        if "text" in entry
                        else base64.b64decode(entry["base64"])
                    )
                if not path.is_symlink():
                    path.chmod(entry["mode"])
                if name.startswith(".cache/nacre/lock-art-old-"):
                    index = int(path.stem.rsplit("-", 1)[1])
                    os.utime(path, (index, index))
            system = base / "sys/class"
            system.mkdir(parents=True)
            (system / "power_supply").symlink_to(
                home / "fixture-sys/power_supply", target_is_directory=True
            )
            env = dict(
                os.environ,
                HOME=str(home),
                XDG_CONFIG_HOME=str(home / ".config"),
                XDG_DATA_HOME=str(home / ".local/share"),
                XDG_STATE_HOME=str(home / ".local/state"),
                XDG_CACHE_HOME=str(home / ".cache"),
                NACRE_LANGUAGE="en",
                NACRE_HELPER_TOOLS=str(ROOT / "nacre/shell-tools"),
            )
            request = dict(
                operation="prepare",
                home=str(home),
                sys=str(base / "sys"),
                now=reference["clock"],
                native_clock=reference["native_clock"],
                system_info=reference["system_info"],
                data=case["data"],
            )
            font_env, _ = setup(base, home, reference["font_inputs"])
            env.update(font_env)
            provider_before = fingerprints(base)
            result = subprocess.run(
                [str(ROOT / "target/debug/examples/lock-parity")],
                input=json.dumps(request),
                env=env,
                capture_output=True,
                text=True,
                timeout=120,
            )
            assert result.returncode == 0, (
                case["name"],
                result.stdout,
                result.stderr[-1000:],
            )
            assert fingerprints(base) == provider_before, (
                "Native helper changed font-provider inputs"
            )
            actual = Files(home, []).snapshot()
            expected = case["outputs"]
            assert set(actual) == set(expected), (
                case["name"],
                set(actual) ^ set(expected),
            )
            for name in expected:
                assert actual[name] == expected[name], f"{case['name']}: {name} differs"
    print(
        f"Native lock preparation: {len(reference['cases'])} cases and every file exact"
    )


if __name__ == "__main__":
    main()
