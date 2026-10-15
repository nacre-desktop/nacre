"""Validated profile composition. No accounts, vault content or device writes."""

import argparse
from nacre_i18n import tr
import copy
import json
import nacre_state as _state
import importlib.util
import os
from pathlib import Path
import re

DEFAULT = dict(
    version=1,
    id="default",
    components=["configs", "shell"],
    workspaceCount=6,
    workspaceNames=[],
    workspaceIcons=[],
    startup=[],
    routes=[],
    brainWorkspace=None,
    aiWorkspace=None,
    aiFullAccess=False,
    windowTrashBindings=False,
)
COMPONENTS = {"configs", "apps", "shell", "brain", "ai"}


def validate(value):
    if not isinstance(value, dict) or set(value) - (set(DEFAULT) | {"aurReview"}):
        raise ValueError(
            tr("helper.profile-policy.unknown-profile-fields", "Unknown profile fields")
        )
    result = {**copy.deepcopy(DEFAULT), **value}
    if result["version"] != 1 or not re.fullmatch(
        r"[a-z0-9][a-z0-9-]{0,39}", str(result["id"])
    ):
        raise ValueError(
            tr(
                "helper.profile-policy.unsupported-profile-identity-version",
                "Unsupported profile identity/version",
            )
        )
    if "aurReview" in result and result["aurReview"] not in ("required", "native"):
        raise ValueError("Invalid registered AUR review policy")
    count = result["workspaceCount"]
    if type(count) is not int or not 1 <= count <= 10:
        raise ValueError(
            tr(
                "helper.profile-policy.workspace-count-must-be-1-10",
                "Workspace count must be 1–10",
            )
        )
    if (
        not isinstance(result["components"], list)
        or set(result["components"]) - COMPONENTS
        or len(set(result["components"])) != len(result["components"])
    ):
        raise ValueError(
            tr(
                "helper.profile-policy.invalid-optional-component-list",
                "Invalid optional component list",
            )
        )
    for name in ("workspaceNames", "workspaceIcons"):
        if (
            not isinstance(result[name], list)
            or len(result[name]) > count
            or any(
                not isinstance(v, str) or len(v) > 100 or "\x00" in v
                for v in result[name]
            )
        ):
            raise ValueError(
                tr(
                    "helper.profile-policy.invalid-workspace-labels-icons",
                    "Invalid workspace labels/icons",
                )
            )
    for name in ("brainWorkspace", "aiWorkspace"):
        if result[name] is not None and (
            type(result[name]) is not int or not 1 <= result[name] <= count
        ):
            raise ValueError(
                tr(
                    "helper.profile-policy.invalid-optional-app-workspace",
                    "Invalid optional app workspace",
                )
            )
    if type(result["windowTrashBindings"]) is not bool:
        raise ValueError(
            tr(
                "helper.profile-policy.invalid-window-trash-preference",
                "Window trash binding preference must be a boolean",
            )
        )
    if type(result["aiFullAccess"]) is not bool:
        raise ValueError(
            tr(
                "helper.profile-policy.invalid-assistant-permission-preference",
                "Invalid assistant permission preference",
            )
        )
    for name in ("startup", "routes"):
        if not isinstance(result[name], list) or len(result[name]) > 64:
            raise ValueError(
                tr(
                    "helper.profile-policy.invalid-profile-routes-startup-list",
                    "Invalid profile routes/startup list",
                )
            )
        for item in result[name]:
            expected = (
                {"label", "workspace", "argv", "classes"}
                if name == "startup"
                else {"name", "match", "workspace"}
            )
            if (
                not isinstance(item, dict)
                or set(item) != expected
                or type(item["workspace"]) is not int
                or not 1 <= item["workspace"] <= count
            ):
                raise ValueError(
                    tr(
                        "helper.profile-policy.invalid-profile-entry",
                        "Invalid profile entry",
                    )
                )
            for key in ("label",) if name == "startup" else ("name", "match"):
                if (
                    not isinstance(item[key], str)
                    or not item[key]
                    or len(item[key]) > 200
                    or any(c in item[key] for c in "\x00\n\r")
                ):
                    raise ValueError(
                        tr(
                            "helper.profile-policy.invalid-profile-entry-text",
                            "Invalid profile entry text",
                        )
                    )
            if name == "startup":
                if (
                    not isinstance(item["argv"], list)
                    or not 1 <= len(item["argv"]) <= 32
                    or any(not isinstance(v, str) or "\x00" in v for v in item["argv"])
                ):
                    raise ValueError(
                        tr(
                            "helper.profile-policy.invalid-startup-arguments",
                            "Invalid startup arguments",
                        )
                    )
                if not isinstance(item["classes"], list) or any(
                    not isinstance(v, str) or len(v) > 200 for v in item["classes"]
                ):
                    raise ValueError(
                        tr(
                            "helper.profile-policy.invalid-startup-class-list",
                            "Invalid startup class list",
                        )
                    )
    return result


def load(home=None):
    path = (home or Path.home()) / ".config/nacre/profile.json"
    return (
        validate(_state.read_json_file(path))
        if path.exists()
        else copy.deepcopy(DEFAULT)
    )


def selected():
    path = os.environ.get("NACRE_PROFILE_FILE")
    return validate(_state.read_json_file(Path(path))) if path else load()


def lua(profile):
    spec = importlib.util.spec_from_file_location(
        "nacre_compositor_config", Path(__file__).with_name("nacre_native_config.py")
    )
    renderer = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(renderer)
    return "-- Generated by the selected Nacre profile.\n" + (
        renderer.render_profile(profile) or "\n"
    )


def install(home=None):
    home = home or Path.home()
    value = selected()
    directory = home / ".config/nacre"
    directory.mkdir(parents=True, exist_ok=True)
    for name, text in (
        ("profile.json", json.dumps(value, indent=2) + "\n"),
        ("profile.lua", lua(value)),
    ):
        target = directory / name
        if _state.write_text_if_managed(target, text):
            continue
        pending = target.with_name("." + name + ".next")
        pending.write_text(text)
        pending.chmod(0o600)
        os.replace(pending, target)
    return value


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "action", choices=("show", "install"), default="show", nargs="?"
    )
    args = parser.parse_args()
    print(json.dumps(install() if args.action == "install" else selected()))
