"""Niri transport, real event schema and shared facade action semantics."""

import copy
import importlib.util
import json
import os
from pathlib import Path
import socket
import threading
import unittest
from unittest.mock import patch

import compositor

spec = importlib.util.spec_from_file_location(
    "niri_backend_test",
    Path(compositor.__file__).with_name("compositor_backends") / "niri.py",
)
niri = importlib.util.module_from_spec(spec)
spec.loader.exec_module(niri)
FIXTURE = json.loads(
    (Path(__file__).parent / "fixtures/niri-event-stream.json").read_text()
)


def state():
    result = niri.State(FIXTURE["outputs"])
    for event in FIXTURE["events"][:2]:
        result.apply(event)
    return result


class NiriStateTests(unittest.TestCase):
    def test_overview_hover_preserves_focus_and_u64_identity_then_clears(self):
        model = state()
        focused = model.snapshot()["activeWindow"]["windowId"]
        model.apply({"OverviewOpenedOrClosed": {"is_open": True}})
        model.apply({"NacreOverviewHoverChanged": {"id": 9007199254740995}})
        snapshot = model.snapshot()
        self.assertTrue(snapshot["overviewHoverSupported"])
        self.assertEqual(
            snapshot["overviewHoveredWindowId"], "niri-window:9007199254740995"
        )
        self.assertEqual(snapshot["activeWindow"]["windowId"], focused)
        model.apply({"OverviewOpenedOrClosed": {"is_open": False}})
        self.assertIsNone(model.snapshot()["overviewHoveredWindowId"])
        model.apply({"NacreOverviewHoverChanged": {"id": 1}})
        self.assertIsNone(model.snapshot()["overviewHoveredWindowId"])
        with self.assertRaises(ValueError):
            model.apply({"NacreOverviewHoverChanged": {"id": True}})

    def test_initial_state_preserves_u64_ids_and_output_relative_indices(self):
        value = state().snapshot()
        self.assertEqual(
            value["activeWorkspace"]["id"], "niri-workspace:9007199254740993"
        )
        self.assertEqual(
            value["activeWindow"]["windowId"], "niri-window:9007199254740995"
        )
        self.assertEqual(value["focusedMonitor"]["name"], "DP-2")
        self.assertEqual(
            [row["index"] for row in value["workspaces"] if row["outputId"] == "DP-2"],
            [1, 2, 3],
        )
        self.assertEqual(
            value["activeWindow"]["geometry"],
            {"x": 28, "y": 44, "width": 500, "height": 400},
        )
        self.assertEqual(value["activeWindow"]["pid"], 123)
        self.assertFalse(value["activeWindow"]["urgent"])
        self.assertFalse(value["activeWindow"]["fullscreenKnown"])

    def test_interactive_move_is_distinct_from_missing_tiled_geometry(self):
        value = state()
        key = "niri-window:9007199254740995"
        original = copy.deepcopy(value.windows[key])
        moved = copy.deepcopy(original)
        moved["workspace_id"] = None
        moved["layout"]["tile_pos_in_workspace_view"] = None
        value.apply({"WindowOpenedOrChanged": {"window": moved}})
        window = next(
            row for row in value.snapshot()["windows"] if row["windowId"] == key
        )
        self.assertTrue(window["moving"])
        self.assertFalse(window["geometryKnown"])
        # Niri also omits some tiled positions; those are not pointer moves.
        moved["workspace_id"] = original["workspace_id"]
        value.apply({"WindowOpenedOrChanged": {"window": moved}})
        window = next(
            row for row in value.snapshot()["windows"] if row["windowId"] == key
        )
        self.assertFalse(window["moving"])
        self.assertFalse(window["geometryKnown"])
        value.apply({"WindowOpenedOrChanged": {"window": original}})
        window = next(
            row for row in value.snapshot()["windows"] if row["windowId"] == key
        )
        self.assertFalse(window["moving"])
        self.assertTrue(window["geometryKnown"])

    def test_inactive_output_activation_does_not_change_focused_workspace(self):
        value = state()
        value.apply(FIXTURE["events"][2])
        snapshot = value.snapshot()
        self.assertEqual(snapshot["focusedMonitor"]["name"], "DP-2")
        self.assertEqual(
            snapshot["monitors"][0]["activeWorkspace"]["id"], "niri-workspace:102"
        )
        self.assertTrue(snapshot["monitors"][1]["activeWorkspace"]["focused"])

    def test_null_focus_urgency_geometry_and_stale_events(self):
        value = state()
        for event in FIXTURE["events"][2:]:
            value.apply(event)
        snapshot = value.snapshot()
        self.assertIsNone(snapshot["activeWindow"])
        self.assertNotIn("niri-window:1", value.windows)
        window = next(
            row
            for row in snapshot["windows"]
            if row["windowId"].endswith("9007199254740995")
        )
        self.assertEqual(window["at"], [44, 64])
        self.assertTrue(window["urgent"])

    def test_incremental_window_updates_use_shared_canonical_event_names(self):
        value = state()
        previous = FIXTURE["events"][1]["WindowsChanged"]["windows"][0]
        result = value.apply(
            {
                "WindowOpenedOrChanged": {
                    "window": {
                        **previous,
                        "title": "Changed",
                        "workspace_id": 202,
                        "is_floating": True,
                    }
                }
            }
        )
        self.assertEqual(result["type"], "window-title")
        self.assertEqual(
            result["changes"], ["window-title", "window-workspace", "window-floating"]
        )
        row = next(
            row
            for row in value.snapshot()["windows"]
            if row["windowId"] == "niri-window:1"
        )
        self.assertEqual(row["workspaceId"], "niri-workspace:202")
        self.assertTrue(row["floating"])

    def test_unknown_future_event_shapes_are_harmless(self):
        value = state()
        initial = value.snapshot()
        for event in (
            "FutureUnit",
            {"FutureTuple": [1, 2]},
            {"FutureRecord": {"extra": True}},
        ):
            self.assertEqual(value.apply(event), {"type": "other"})
            self.assertEqual(value.snapshot(), initial)
        with self.assertRaises(ValueError):
            value.apply({"WindowClosed": []})

    def test_config_failure_remains_in_snapshots_until_successful_reload(self):
        value = state()
        value.apply({"ConfigLoaded": {"failed": True}})
        self.assertTrue(value.snapshot()["configFailed"])
        value.apply({"ConfigLoaded": {"failed": False}})
        self.assertFalse(value.snapshot()["configFailed"])

    def test_workspace_reorder_changes_index_without_changing_identity(self):
        value = state()
        changed = [
            dict(row) for row in FIXTURE["events"][0]["WorkspacesChanged"]["workspaces"]
        ]
        changed[2]["idx"], changed[3]["idx"] = 2, 1
        value.apply({"WorkspacesChanged": {"workspaces": changed}})
        snapshot = value.snapshot()
        self.assertEqual(snapshot["activeWorkspace"]["index"], 2)
        self.assertEqual(
            snapshot["activeWindow"]["workspaceId"], snapshot["activeWorkspace"]["id"]
        )

    def test_removed_workspace_and_output_never_manufacture_geometry(self):
        value = state()
        value.outputs = {"DP-1": FIXTURE["outputs"]["DP-1"]}
        value.apply(
            {
                "WorkspacesChanged": {
                    "workspaces": FIXTURE["events"][0]["WorkspacesChanged"][
                        "workspaces"
                    ][:2]
                }
            }
        )
        window = next(
            row
            for row in value.snapshot()["windows"]
            if row["windowId"].endswith("9007199254740995")
        )
        self.assertFalse(window["geometryKnown"])
        self.assertEqual(window["at"], [None, None])
        self.assertIsNone(window["outputId"])
        self.assertEqual(window["workspaceId"], "niri-workspace:9007199254740993")


class NiriActionTests(unittest.TestCase):
    def setUp(self):
        self.backend = niri.Backend()
        self.snapshot = state().snapshot()
        self.cached = patch.object(self.backend, "_state", return_value=self.snapshot)
        self.cached.start()
        self.addCleanup(self.cached.stop)
        self.transport = patch.object(self.backend, "_request", return_value="Handled")
        self.request = self.transport.start()
        self.addCleanup(self.transport.stop)

    def test_numbers_resolve_on_focused_output_and_explicit_refs_use_stable_ids(self):
        self.backend.switch_workspace(1)
        self.request.assert_called_with(
            {"Action": {"FocusWorkspace": {"reference": {"Id": 9007199254740993}}}},
            timeout=10,
        )
        self.backend.switch_workspace({"output": "DP-1", "index": 1})
        self.assertEqual(
            self.request.call_args.args[0]["Action"]["FocusWorkspace"]["reference"],
            {"Id": 101},
        )
        self.backend.switch_workspace({"id": "niri-workspace:202"})
        self.assertEqual(
            self.request.call_args.args[0]["Action"]["FocusWorkspace"]["reference"],
            {"Id": 202},
        )
        self.backend.switch_workspace({"name": "coding"})
        self.assertEqual(
            self.request.call_args.args[0]["Action"]["FocusWorkspace"]["reference"],
            {"Id": 9007199254740993},
        )

    def test_relative_indices_and_scrolling_direction_actions(self):
        self.backend.switch_workspace({"relative": 1})
        self.assertEqual(
            self.request.call_args.args[0]["Action"]["FocusWorkspace"]["reference"],
            {"Id": 202},
        )
        with self.assertRaises(ValueError):
            self.backend.switch_workspace({"relative": -1})
        self.backend.focus_direction("left")
        self.assertEqual(
            self.request.call_args.args[0], {"Action": {"FocusColumnLeft": {}}}
        )
        self.backend.move_direction("down")
        self.assertEqual(
            self.request.call_args.args[0], {"Action": {"MoveWindowDown": {}}}
        )

    def test_missing_workspace_and_invalid_ids_cannot_dispatch(self):
        for value in (
            True,
            None,
            9,
            {"output": "missing", "index": 1},
            {"id": "niri-workspace:404"},
        ):
            with self.assertRaises(ValueError):
                self.backend.switch_workspace(value)
        for value in (
            "0xabc",
            "niri-window:1;exit",
            "niri-window:18446744073709551616",
            "niri-window:01",
        ):
            with self.assertRaises(ValueError):
                self.backend.focus_window(value)
        self.request.assert_not_called()

    def test_focus_close_move_float_resize_and_maximize_contract(self):
        self.backend.focus_window("niri-window:1", timeout=3)
        self.backend.close_window("niri-window:1", timeout=3)
        self.backend.move_window("niri-window:1", workspace=2, follow=False, timeout=3)
        self.backend.float_window("niri-window:1", action="enable", timeout=3)
        self.backend.resize_window("niri-window:1", 600, 400, timeout=3)
        self.backend.fullscreen_window("niri-window:1", mode="maximized", timeout=3)
        actions = [call.args[0]["Action"] for call in self.request.call_args_list]
        self.assertEqual(
            [next(iter(action)) for action in actions],
            [
                "FocusWindow",
                "CloseWindow",
                "MoveWindowToWorkspace",
                "MoveWindowToFloating",
                "SetWindowWidth",
                "SetWindowHeight",
                "MaximizeWindowToEdges",
            ],
        )
        self.assertEqual(
            actions[2]["MoveWindowToWorkspace"],
            {"window_id": 1, "reference": {"Id": 202}, "focus": False},
        )
        self.assertTrue(
            all(call.kwargs["timeout"] == 3 for call in self.request.call_args_list)
        )

    def test_global_placement_uses_measured_delta_not_workarea_assumptions(self):
        self.backend.place_window(
            "niri-window:9007199254740995",
            x=100,
            y=200,
            width=500,
            height=400,
            timeout=3,
        )
        actions = [call.args[0]["Action"] for call in self.request.call_args_list]
        self.assertEqual(
            [next(iter(action)) for action in actions],
            [
                "MoveWindowToFloating",
                "SetWindowWidth",
                "SetWindowHeight",
                "MoveFloatingWindow",
            ],
        )
        self.assertEqual(
            actions[-1]["MoveFloatingWindow"],
            {
                "id": 9007199254740995,
                "x": {"AdjustFixed": 72},
                "y": {"AdjustFixed": 156},
            },
        )

    def test_geometry_rejects_tiled_unknown_and_nonfinite_positions(self):
        with self.assertRaises(ValueError):
            self.backend.move_window("niri-window:1", x=100, y=100)
        for bad in (float("nan"), float("inf"), True):
            with self.assertRaises(ValueError):
                self.backend.move_window("niri-window:1", x=bad, y=100)
        self.snapshot["activeWindow"]["geometryKnown"] = False
        with self.assertRaises(RuntimeError):
            self.backend.move_window("niri-window:9007199254740995", x=100, y=100)
        self.request.assert_not_called()

    def test_cross_output_move_requeries_geometry_before_measured_adjustment(self):
        row = self.snapshot["activeWindow"]
        moved = {
            **row,
            "outputId": "DP-1",
            "geometry": {"x": -1892, "y": 44, "width": 500, "height": 400},
        }
        with patch.object(self.backend, "_window", side_effect=[row, moved]):
            self.backend.move_window(row["windowId"], x=-1800, y=100)
        self.assertEqual(
            self.request.call_args_list[0].args[0]["Action"]["MoveWindowToMonitor"][
                "output"
            ],
            "DP-1",
        )
        self.assertEqual(
            self.request.call_args_list[1].args[0]["Action"]["MoveFloatingWindow"]["x"],
            {"AdjustFixed": 92},
        )

    def test_unsupported_capabilities_fail_without_hyprland_or_fake_success(self):
        self.assertFalse(self.backend.capabilities["pinWindow"])
        for method in (
            self.backend.pin_window,
            self.backend.kill_window,
            self.backend.set_cursor,
        ):
            with self.assertRaises(RuntimeError):
                method("niri-window:1")
        self.request.assert_not_called()


class NiriTransportTests(unittest.TestCase):
    def test_optional_hover_stream_falls_back_to_stock_without_consuming_events(self):
        clients = []
        servers = []
        for response in (
            b'{"Err":"unknown request"}\n',
            b'{"Ok":"Handled"}\n{"OverviewOpenedOrClosed":{"is_open":true}}\n',
        ):
            client, server = socket.socketpair()
            clients.append(client)
            servers.append(server)
            server.sendall(response)
            self.addCleanup(server.close)
            self.addCleanup(client.close)
        backend = niri.Backend()
        with (
            patch.dict(os.environ, NACRE_NIRI_HOVER_STREAM="1"),
            patch.object(backend, "_connection", side_effect=clients),
        ):
            connection = backend.event_socket()
            self.assertIs(connection, clients[1])
            self.assertEqual(
                json.loads(servers[0].recv(1024)), "NacreOverviewHoverStream"
            )
            self.assertEqual(json.loads(servers[1].recv(1024)), "EventStream")
            self.assertIn(b"OverviewOpenedOrClosed", connection.recv(1024))

    def exchange(self, reply):
        client, server = socket.socketpair()
        received = []

        def responder():
            with server, server.makefile("rb") as stream:
                received.append(json.loads(stream.readline()))
                # Exercise partial frames: transport must wait for a newline.
                data = reply.encode()
                server.sendall(data[:4])
                server.sendall(data[4:])

        thread = threading.Thread(target=responder)
        thread.start()
        self.addCleanup(thread.join)
        backend = niri.Backend()
        return (
            backend,
            patch.object(backend, "_connection", return_value=client),
            received,
        )

    def test_native_socket_framing_and_errors(self):
        backend, connection, received = self.exchange('{"Ok":"Handled"}\n')
        with connection:
            backend.close_window("niri-window:9007199254740995")
        self.assertEqual(
            received, [{"Action": {"CloseWindow": {"id": 9007199254740995}}}]
        )
        backend, connection, _ = self.exchange('{"Err":"window no longer exists"}\n')
        with connection, self.assertRaisesRegex(RuntimeError, "no longer exists"):
            backend.close_window("niri-window:1")

    def test_stream_initial_state_updates_then_disconnect_are_explicit(self):
        client, server = socket.socketpair()
        server.sendall(
            b"".join((json.dumps(event) + "\n").encode() for event in FIXTURE["events"])
        )
        server.close()
        backend = niri.Backend()
        with (
            patch.object(backend, "event_socket", return_value=client),
            patch.object(backend, "_response", return_value=FIXTURE["outputs"]),
        ):
            stream = backend.events()
            initial = next(stream)
            self.assertEqual(initial["type"], "snapshot")
            self.assertEqual(len(initial["windows"]), 3)
            values = []
            with self.assertRaises(ConnectionError):
                while True:
                    values.append(next(stream))
            final = next(
                value for value in reversed(values) if value["type"] == "snapshot"
            )
            self.assertEqual(len(final["windows"]), 2)
            self.assertIsNone(final["activeWindow"])
        self.assertEqual(client.fileno(), -1)

    def test_output_metadata_changes_emit_the_shared_outputs_event(self):
        client, server = socket.socketpair()
        source = [*FIXTURE["events"][:2], {"ConfigLoaded": {"failed": False}}]
        server.sendall(
            b"".join((json.dumps(event) + "\n").encode() for event in source)
        )
        server.close()
        changed = copy.deepcopy(FIXTURE["outputs"])
        changed["DP-2"]["logical"]["x"] = 200
        backend = niri.Backend()
        with (
            patch.object(backend, "event_socket", return_value=client),
            patch.object(
                backend,
                "_response",
                side_effect=[FIXTURE["outputs"], FIXTURE["outputs"], changed],
            ),
        ):
            values = []
            with self.assertRaises(ConnectionError):
                for value in backend.events():
                    values.append(value)
        self.assertIn({"type": "outputs-changed"}, values)
        final = next(value for value in reversed(values) if value["type"] == "snapshot")
        self.assertEqual(
            next(row for row in final["monitors"] if row["name"] == "DP-2")["x"], 200
        )
        self.assertEqual(client.fileno(), -1)

    def test_selection_is_explicit_and_import_starts_no_process_or_socket(self):
        self.assertEqual(compositor.select_backend({}), "hyprland")
        self.assertEqual(compositor.select_backend({"NIRI_SOCKET": "/fixture"}), "niri")
        self.assertEqual(
            compositor.select_backend(
                {"NIRI_SOCKET": "/fixture", "HYPRLAND_INSTANCE_SIGNATURE": "fixture"}
            ),
            "hyprland",
        )
        self.assertEqual(
            compositor.select_backend(
                {
                    "XDG_CURRENT_DESKTOP": "niri",
                    "HYPRLAND_INSTANCE_SIGNATURE": "fixture",
                }
            ),
            "niri",
        )
        with (
            patch.dict(os.environ, {"NACRE_COMPOSITOR": "niri"}),
            patch.object(socket, "socket") as connection,
            patch.object(niri.subprocess, "run") as process,
        ):
            spec = importlib.util.spec_from_file_location(
                "isolated_niri_facade", Path(compositor.__file__)
            )
            module = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(module)
            self.assertEqual(module.backend_name, "niri")
            connection.assert_not_called()
            process.assert_not_called()


if __name__ == "__main__":
    unittest.main()
