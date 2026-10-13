"""Neutral configuration source and explicit backend compiler selection.

Installed profiles and repository tooling use this boundary. Hyprland lowering
and executable Lua callbacks belong to compositor_backends; a future backend
can consume the same action/selector declarations without importing Hyprland.
"""

import importlib.util
import json
from pathlib import Path

DIRECTORY = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location(
    "nacre_hyprland_config", DIRECTORY / "compositor_backends/hyprland_config.py"
)
hyprland = importlib.util.module_from_spec(spec)
spec.loader.exec_module(hyprland)
niri_spec = importlib.util.spec_from_file_location(
    "nacre_niri_config", DIRECTORY / "compositor_backends/niri_config.py"
)
niri = importlib.util.module_from_spec(niri_spec)
niri_spec.loader.exec_module(niri)
OUTPUTS = hyprland.OUTPUTS


def supported_backends():
    return ("hyprland", "niri")


def backend_capabilities(name):
    if name not in supported_backends():
        raise ValueError("Unknown compositor backend")
    return {
        "backend": name,
        "backendReady": name == "hyprland",
        "setup_ready": name == "hyprland",
        "managedIncludes": name == "niri",
        "minimumVersion": "26.04" if name == "niri" else None,
        "includeMinimumVersion": "25.11" if name == "niri" else None,
        "testedVersion": "26.04" if name == "niri" else None,
        "generatedFiles": list(niri.OUTPUTS if name == "niri" else OUTPUTS),
    }


def render(contract, directory=DIRECTORY, backend="hyprland"):
    if backend == "niri":
        # Reuse the existing strict declaration validation. Compiling data does
        # not execute callbacks or write the discarded Hyprland output.
        hyprland.render(contract, directory)
        return niri.render(contract, directory)
    if backend != "hyprland":
        raise ValueError("Unsupported compositor backend: " + backend)
    return hyprland.render(contract, directory)


def load(directory=DIRECTORY):
    return json.loads((directory / "compositor-contract.json").read_text())


def rule_lua(rule, backend="hyprland"):
    if backend != "hyprland":
        raise ValueError("Unsupported compositor backend: " + backend)
    return hyprland.rule_lua(rule)


def workspace_target(value):
    return hyprland.workspace_target(value)


def profile_rules(profile):
    """Profile opt-ins use the same neutral rule source; defaults emit none."""
    rules = [
        {
            "kind": "window-rule",
            "id": "profile-" + route["name"],
            "selector": {"app_id_regex": route["match"]},
            "effects": {
                "workspace": {
                    "target": {"kind": "number", "number": route["workspace"]},
                    "activate": False,
                }
            },
        }
        for route in profile["routes"]
    ]
    if profile["brainWorkspace"] is not None:
        rules.append(
            {
                "kind": "window-rule",
                "id": "profile-brain",
                "selector": {
                    "app_id_regex": "^(nacre-brain|chrome-127[.]0[.]0[.]1__-Default|[Oo]bsidian)$"
                },
                "effects": {
                    "workspace": {
                        "target": {
                            "kind": "number",
                            "number": profile["brainWorkspace"],
                        },
                        "activate": False,
                    }
                },
            }
        )
    return rules


def rule_kdl(rule):
    return niri.rule_kdl(rule)


def render_keyboard(values, backend="hyprland"):
    if backend not in supported_backends():
        raise ValueError("Unknown compositor backend")
    spec = importlib.util.spec_from_file_location(
        "nacre_keyboard_backend", DIRECTORY / "compositor_backends" / (backend + ".py")
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module.Backend().render_keyboard(values)


def render_profile(profile, backend="hyprland", workspace_names=None):
    rules = profile_rules(profile)
    if backend == "hyprland":
        text = "\n".join(rule_lua(rule) for rule in rules) + ("\n" if rules else "")
        if profile.get("windowTrashBindings", False):
            contract = load()
            bindings = contract.pop("profileBindings")["windowTrashBindings"]
            contract["outputs"] = {name: [] for name in OUTPUTS}
            contract["outputs"]["hypr/conf/keybinding.lua"] = bindings
            text += render(contract)["hypr/conf/keybinding.lua"]
        return text
    if backend != "niri":
        raise ValueError("Unknown compositor backend")
    workspace_names = workspace_names or {}
    for rule in rules:
        target = rule["effects"]["workspace"]["target"]
        number = target.get("number")
        if number not in workspace_names or not isinstance(
            workspace_names[number], str
        ):
            raise ValueError(
                "Niri profile routes require an explicit workspace number-to-name map"
            )
        rule["effects"]["workspace"]["target"] = {
            "kind": "name",
            "name": workspace_names[number],
        }
    return "\n".join(rule_kdl(rule) for rule in rules) + ("\n" if rules else "")
