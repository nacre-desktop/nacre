"""Protect lock privacy, glass compositing and shared native output geometry."""

import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("dashboard", ROOT / "lock-dashboard.py")
dashboard = importlib.util.module_from_spec(spec)
spec.loader.exec_module(dashboard)
spec = importlib.util.spec_from_file_location("lockconfig", ROOT / "lock-config.py")
config = importlib.util.module_from_spec(spec)
spec.loader.exec_module(config)


class LockDashboardTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.colors = json.loads((ROOT / "reference-style.json").read_text())["colours"]

    def test_hidden_messages_cannot_change_the_rendered_panel(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            first = dict(
                notifications=[
                    dict(app="Unknown app", summary="Secret", body="Private")
                ]
            )
            second = dict(
                notifications=[
                    dict(
                        app="Unknown app",
                        summary="Different secret",
                        body="Different private",
                    )
                ]
            )
            with patch.object(
                dashboard,
                "system_info",
                return_value=dict(os="Linux", user="Test", uptime="1h"),
            ):
                a = dashboard.render(first, self.colors, "", None, home)
                b = dashboard.render(second, self.colors, "", None, home)
                self.assertEqual(a.tobytes(), b.tobytes())
                first["preferences"] = dict(lockNotificationContents=True)
                c = dashboard.render(first, self.colors, "", None, home)
                self.assertNotEqual(
                    a.crop((1944, 416, 2848, 1588)).tobytes(),
                    c.crop((1944, 416, 2848, 1588)).tobytes(),
                )
                self.assertEqual(a.getpixel((60, 60))[3], 255)
                self.assertEqual(a.getpixel((0, 0))[3], 0)

    def test_geometry_identity_ignores_workspace_and_scale(self):
        base = dict(name="eDP-1", width=2880, height=1800, transform=0)
        self.assertEqual(
            config.panel_key(base),
            config.panel_key(dict(base, activeWorkspace={"id": 7}, scale=1.5)),
        )
        self.assertEqual(dashboard.layout(base), config.panel_scale(base))
        self.assertEqual(dashboard.layout(dict(base, transform=1)), 1800 / 2048)

    def test_missing_art_and_weather_publish_ready_private_files(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            monitor = dict(name="test", width=1920, height=1080)
            rows = dashboard.publish(
                dict(colors=self.colors), self.colors, "/missing", None, home, [monitor]
            )
            path = Path(rows[0]["path"])
            self.assertTrue(path.is_file())
            self.assertEqual(path.stat().st_mode & 0o777, 0o600)
            self.assertEqual(
                Path(rows[0]["pointer"]).name,
                "dashboard-" + config.panel_key(monitor) + ".txt",
            )
            metadata = json.loads(
                (home / ".cache/nacre/lock-ready/dashboard.json").read_text()
            )
            self.assertEqual(metadata["panel"], str(path))
            self.assertNotIn("password", metadata)
            initial = path.parent / (
                "dashboard-" + config.panel_key(monitor) + "-initial.png"
            )
            self.assertEqual(initial.read_bytes(), path.read_bytes())
            # Pruning obsolete immutable reload paths must not invalidate the
            # startup path embedded in a config generated hours earlier.
            for i in range(20):
                (path.parent / f"dashboard-old-{i}.png").write_bytes(b"old")
            dashboard.publish({}, self.colors, "/missing", None, home, [monitor])
            self.assertTrue(initial.is_file())
            immutable = [
                p
                for p in path.parent.glob("dashboard-*-*.png")
                if not p.stem.endswith("-initial")
            ]
            self.assertLessEqual(len(immutable), 12)

    def test_concurrent_publishers_leave_complete_startup_and_reload_images(self):
        from concurrent.futures import ThreadPoolExecutor
        from PIL import Image

        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            monitor = dict(name="test", width=1920, height=1080)

            def publish(index):
                return dashboard.publish(
                    dict(count=index), self.colors, "", None, home, [monitor]
                )

            with ThreadPoolExecutor(max_workers=2) as workers:
                rows = list(workers.map(publish, (1, 2)))
            pointer = Path(rows[0][0]["pointer"])
            current = Path(pointer.read_text())
            with Image.open(current) as image:
                self.assertEqual(image.size, (2880, 1620))
            initial = pointer.with_name(pointer.stem + "-initial.png")
            self.assertEqual(initial.read_bytes(), current.read_bytes())

    def test_icon_resolver_rejects_paths_and_urls(self):
        with tempfile.TemporaryDirectory() as folder:
            for name in (
                "../../secret",
                "https://host/private",
                "/home/user/private.png",
            ):
                self.assertIsNone(
                    dashboard.notification_icon(
                        name, "Not an installed app", Path(folder)
                    )
                )
