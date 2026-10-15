"""Developer-only mapping of historical patched media paths to native methods."""

from functools import partial
import time
import json
import tempfile
from unittest.mock import Mock
from pathlib import Path
from native_lock_dashboard import invoke

METHODS = (
    "settings",
    "media_state",
    "palette_presets",
    "preference",
    "previews",
    "describe",
    "catalog",
    "palette_marker",
    "wallpaper_mode",
    "theme",
    "select",
    "warm",
    "prepared_theme",
    "demo_start",
    "demo_restore",
    "demo_scenes",
    "atomic",
    "import_files",
    "read",
)


def call(media, method, *args, **kwargs):
    if method == "warm" and not args:
        args = (kwargs.get("include_demo", True),)
    if method == "prepared_theme" and len(args) == 1:
        args = (*args, kwargs.get("live", True))
    if method == "palette_marker":
        names = ("poster", "flavour", "harmony", "source", "mode", "personality")
        args = tuple(args) + tuple(kwargs.get(name) for name in names[len(args) :])
    request = dict(
        method=method,
        args=[str(v) if hasattr(v, "__fspath__") else v for v in args],
        home=str(media.HOME),
        library=str(media.LIBRARY),
        cache=str(media.CACHE),
        state=str(media.STATE),
        prefs=str(media.PREFS),
        now_ms=int(time.time() * 1000),
    )
    spy = getattr(getattr(media, "subprocess", None), "run", None)
    with tempfile.TemporaryDirectory(prefix="nacre-media-command-fixture-") as folder:
        log = Path(folder) / "commands.jsonl"
        if isinstance(spy, Mock) and method in ("theme", "select", "warm"):
            request["command_log"] = str(log)
            if isinstance(spy.side_effect, BaseException) or (
                isinstance(spy.side_effect, type)
                and issubclass(spy.side_effect, BaseException)
            ):
                request["command_error"] = str(spy.side_effect)
            if method == "warm" and spy._mock_wraps:
                cli = media.HOME / ".local/share/nacre/shell/bin/nacre_shell"
                poster = media.media_state().get("poster", "")
                response = spy._mock_wraps(
                    [str(cli), "wallpaper", "-p", poster],
                    capture_output=True,
                    text=True,
                    check=True,
                )
                request["command_palette"] = json.loads(response.stdout)
        publisher = (
            media.demo_module("classic-state") if method == "demo_restore" else None
        )
        publication = Path(folder) / "publication.jsonl"
        publication_error = None
        if publisher and isinstance(publisher.apply_palette, Mock):
            request["publication_log"] = str(publication)
            effect = publisher.apply_palette.side_effect
            if effect is not None:
                publication_error = next(effect)
                request["publication_error"] = str(publication_error)
        state_override = None
        if method == "warm" and isinstance(media.media_state, Mock):
            state_override = media.STATE / "media.json"
            previous_state = (
                state_override.read_bytes() if state_override.exists() else None
            )
            state_override.parent.mkdir(parents=True, exist_ok=True)
            state_override.write_text(json.dumps(media.media_state()))
        native_error = None
        try:
            result = invoke("media", **request)
        except ValueError as error:
            native_error = error
        finally:
            if state_override is not None:
                if previous_state is None:
                    state_override.unlink(missing_ok=True)
                else:
                    state_override.write_bytes(previous_state)
        if log.exists():
            for line in log.read_text().splitlines():
                command = json.loads(line)
                if method == "select":
                    spy(
                        command,
                        check=True,
                        stdout=media.subprocess.DEVNULL,
                        stderr=media.subprocess.PIPE,
                        timeout=60,
                    )
                else:
                    spy(
                        command,
                        check=True,
                        capture_output=True,
                        text=True,
                        timeout=30 if method == "warm" else 60,
                    )
        if publication.exists():
            for index, line in enumerate(publication.read_text().splitlines()):
                if index == 0 and publication_error is not None:
                    # The first failure was supplied to Rust; expose the same observed call.
                    publisher.apply_palette.side_effect = None
                    publisher.apply_palette(media.HOME, json.loads(line), live=True)
                    publisher.apply_palette.side_effect = effect
                else:
                    publisher.apply_palette(media.HOME, json.loads(line), live=True)
            if publication_error is not None:
                if native_error is None or str(native_error) != str(publication_error):
                    raise AssertionError(
                        "Native demo rollback did not propagate publication failure"
                    )
                raise publication_error
        if native_error:
            raise native_error

    return Path(result) if method == "palette_marker" else result


def install(media):
    for method in METHODS:
        setattr(media, method, partial(call, media, method))
    return media
