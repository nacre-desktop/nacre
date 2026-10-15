"""Developer reference API for the pinned Rust engine; never installed as a runtime."""

from ._backend import ENGINE_ID as ENGINE_ID, FORMAT_VERSION as FORMAT_VERSION, __version__ as __version__


def main():
    import os
    from pathlib import Path
    import subprocess
    import sys

    root = Path(__file__).resolve().parents[4]
    override = os.environ.get("NACRE_PALETTE_CLI_PARITY")
    examples = (
        [Path(override)]
        if override
        else [root / "target/debug/examples/palette-cli-parity", root / "target/release/examples/palette-cli-parity"]
    )
    available = [path for path in examples if path.is_file()]
    if available:
        program = Path(sys.executable).name + " -m nacre_shell"
        binary = max(available, key=lambda path: path.stat().st_mtime_ns)
        return subprocess.run([str(binary), program, *sys.argv[1:]], check=False).returncode
    helper = Path(
        os.environ.get("NACRE_HELPER_BINARY", str(Path.home() / ".local/share/nacre/shell/tools/nacre-helper"))
    )
    return subprocess.run([str(helper), "nacre_shell", *sys.argv[1:]], check=False).returncode


def __getattr__(name):
    if name == "cli":
        import importlib.util
        from pathlib import Path

        root = Path(__file__).resolve().parents[4]
        source = root / "nacre/shell-tools/tests/native_palette_cli.py"
        import sys

        sys.path.insert(0, str(source.parent))
        spec = importlib.util.spec_from_file_location("native_palette_cli", source)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        return module
    raise AttributeError(name)
