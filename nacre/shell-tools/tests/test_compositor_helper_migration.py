"""Helper facade boundaries keep explicit intent, fallback behavior and IPC budgets."""

import importlib.util
import io
import os
from pathlib import Path
import subprocess
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch

import native_hyprland as compositor

ROOT = Path(__file__).parents[1]


def load(name):
    spec = importlib.util.spec_from_file_location(
        name.replace("-", "_"), ROOT / (name + ".py")
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class HelperCompositorMigrationTests(unittest.TestCase):
    def test_backend_environment_preserves_explicit_desktop_and_unique_instance_policy(
        self,
    ):
        backend = compositor.backend
        with (
            patch.object(
                backend,
                "query_result",
                return_value=subprocess.CompletedProcess(
                    [], 0, '[{"instance":"selected"}]'
                ),
            ) as query,
            patch.dict(os.environ, {"HYPRLAND_INSTANCE_SIGNATURE": "original"}),
        ):
            self.assertEqual(compositor.session_id(), "original")
            environment = {"XDG_CURRENT_DESKTOP": "custom"}
            self.assertEqual(
                compositor.environment_updates(environment),
                {"HYPRLAND_INSTANCE_SIGNATURE": "selected"},
            )
            self.assertEqual(environment, {"XDG_CURRENT_DESKTOP": "custom"})
        query.assert_called_once_with("instances", timeout=None, check=False)
        for result in (
            subprocess.CompletedProcess([], 1, ""),
            subprocess.CompletedProcess([], 0, "[]"),
            subprocess.CompletedProcess([], 0, "[{},{}]"),
        ):
            with patch.object(backend, "query_result", return_value=result):
                self.assertEqual(
                    compositor.environment_updates({}),
                    {"XDG_CURRENT_DESKTOP": "Hyprland"},
                )

    def test_performance_and_animation_semantics_are_translated_only_by_backend(self):
        with patch.object(compositor.backend, "set_options") as apply:
            compositor.set_animations(True)
            apply.assert_called_once_with({"animations": {"enabled": True}}, timeout=10)
            apply.reset_mock()
            compositor.set_performance(True)
            self.assertEqual(apply.call_args.args[0]["decoration"]["rounding"], 0)
            self.assertEqual(apply.call_args.args[0]["general"]["gaps_out"], 0)
        with patch.object(compositor.backend, "reload") as reload:
            compositor.set_performance(False)
            reload.assert_called_once_with(timeout=10)

    def test_nonempty_explicit_startup_routes_only_its_selected_apps(self):
        startup = load("startup-apps")
        profile = {
            "startup": [
                {
                    "label": "Browser",
                    "workspace": 3,
                    "argv": ["example-browser"],
                    "classes": ["example-browser"],
                }
            ]
        }
        session = SimpleNamespace(readiness=Mock())
        with (
            tempfile.TemporaryDirectory() as folder,
            patch.dict(os.environ, {"XDG_RUNTIME_DIR": folder}),
            patch.object(startup.POLICY, "load", return_value=profile),
            patch.dict(startup.compositor.capabilities, {"perOutputWorkspaces": False}),
            patch.object(
                startup, "helper_binary", return_value=Path("/fixture/nacre-helper")
            ),
            patch.object(
                startup.subprocess,
                "run",
                side_effect=lambda *_, **__: session.readiness(),
            ) as ready,
            patch.object(startup.shutil, "which", return_value="/fake/example-browser"),
            patch.object(
                startup.compositor,
                "query",
                side_effect=[[], [{"class": "example-browser", "title": "Browser"}]],
            ) as query,
            patch.object(startup.compositor, "switch_workspace") as switch,
            patch.object(startup.subprocess, "Popen") as launch,
        ):
            startup.main()
        self.assertEqual(query.call_count, 2)
        query.assert_called_with("windows", timeout=None)
        self.assertEqual([call.args[0] for call in switch.call_args_list], [3, 1])
        switch.assert_called_with(1, current_monitor=True, timeout=None)
        launch.assert_called_once()
        self.assertEqual(launch.call_args.args[0][-1], "example-browser")
        session.readiness.assert_called_once_with()
        ready.assert_called_once_with(
            ["/fixture/nacre-helper", "session-watch", "ready"],
            check=True,
            stdout=subprocess.DEVNULL,
        )

    def test_readiness_failure_never_launches_selected_apps(self):
        startup = load("startup-apps")
        with (
            tempfile.TemporaryDirectory() as folder,
            patch.dict(os.environ, {"XDG_RUNTIME_DIR": folder}),
            patch.object(startup.POLICY, "load", return_value={"startup": [{}]}),
            patch.object(
                startup, "helper_binary", return_value=Path("/fixture/nacre-helper")
            ),
            patch.object(
                startup.subprocess,
                "run",
                side_effect=subprocess.CalledProcessError(1, "readiness"),
            ),
            patch.object(startup.subprocess, "Popen") as launch,
            patch.object(startup.compositor, "query") as query,
        ):
            with self.assertRaises(subprocess.CalledProcessError):
                startup.main()
        launch.assert_not_called()
        query.assert_not_called()

    def test_saved_workflow_applies_only_connected_outputs(self):
        workflows = load("workflow-profiles")
        connected = {"name": "DP-1", "width": 1920, "height": 1080}
        disconnected = {"name": "gone", "width": 1920, "height": 1080}
        with (
            patch.object(
                workflows,
                "load",
                return_value={
                    "meeting": {"displays": [connected, disconnected], "roles": []}
                },
            ),
            patch.object(
                workflows, "devices", return_value={"sinks": [], "sources": []}
            ),
            patch.object(
                workflows.compositor, "query", return_value=[connected]
            ) as query,
            patch.object(workflows.compositor, "configure_monitors") as configure,
            patch.object(workflows.subprocess, "Popen") as launch,
        ):
            workflows.apply("meeting")
        query.assert_called_once_with("monitors", timeout=None)
        configure.assert_called_once_with([connected], timeout=None)
        launch.assert_not_called()

    def test_application_isolation_does_not_migrate_apps_when_no_clients_exist(self):
        isolate = load("isolate-apps")
        with (
            patch.object(isolate.compositor, "query", return_value=[]) as query,
            patch.object(isolate.Path, "iterdir", return_value=[]),
            patch.object(isolate.subprocess, "run") as migrate,
            patch("sys.stdout", new=io.StringIO()),
        ):
            isolate.main()
        query.assert_called_once_with("windows", timeout=None)
        migrate.assert_not_called()
