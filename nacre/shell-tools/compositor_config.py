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
OUTPUTS = hyprland.OUTPUTS


def render(contract, directory=DIRECTORY, backend="hyprland"):
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
