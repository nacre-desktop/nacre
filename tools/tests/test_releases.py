import importlib.util
import fcntl
import os
import shutil
import subprocess
import tempfile
import unittest
from contextlib import nullcontext
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock, patch

spec = importlib.util.spec_from_file_location(
    "release", Path(__file__).parents[1] / "releases.py"
)
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)


class ReleaseTests(unittest.TestCase):
    @unittest.skipUnless(
        os.environ.get("NACRE_HELPER_BINARY"),
        "desktop CI supplies the candidate native helper",
    )
    def test_session_refresh_survives_checkout_removal_and_regenerates_volume_keys(
        self,
    ):
        from importlib.util import module_from_spec, spec_from_file_location

        root = Path(__file__).parents[2]
        install_spec = spec_from_file_location(
            "session_install", root / "nacre/shell-tools/install.py"
        )
        installer = module_from_spec(install_spec)
        install_spec.loader.exec_module(installer)

        with tempfile.TemporaryDirectory() as folder:
            temp = Path(folder)
            home = temp / "home"
            checkout = temp / "checkout-a"
            checkout.mkdir()
            for name in (
                "ai/systemd",
                "bin",
                "brain",
                "fastfetch",
                "fish",
                "hypr",
                "kitty",
                "nacre/desktop",
                "nacre/shell",
                "nacre/shell-tools",
                "niri",
                "tools/patches",
                "uwsm",
            ):
                source = root / name
                if source.is_dir():
                    shutil.copytree(
                        source,
                        checkout / name,
                        ignore=shutil.ignore_patterns("tests", "__pycache__"),
                    )
            shutil.copy2(root / "VERSION", checkout / "VERSION")

            shell = home / ".local/share/nacre/shell"
            tools = shell / "tools"
            shutil.copytree(checkout / "nacre/shell", shell / "source")
            shutil.copytree(checkout / "nacre/shell-tools", tools)
            helper = Path(os.environ["NACRE_HELPER_BINARY"])
            shutil.copy2(helper, tools / "nacre-helper")
            with (
                patch.object(installer, "DEST", shell),
                patch.object(
                    installer, "repo_revision", return_value="installed-revision"
                ),
            ):
                installer.install_session_backend(checkout)
            with (
                patch.object(m, "HOME", home),
                patch.object(m, "CONTROL", home / ".local/share/nacre/control"),
            ):
                m.install_controller(root)

            # A prior deploy marker is intentionally stale and must be ignored.
            (tools / "nacre-backend-root").write_text(str(checkout) + "\n")
            session = home / ".local/share/nacre/niri-on-pc/session"
            (session / "config").mkdir(parents=True)
            (session / "source-revision").write_text("old-revision\n")
            (session / "config/config.kdl").write_text("old generated config\n")
            (session / "stock-niri").write_text("stock Niri\n")
            config = home / ".config/nacre"
            config.mkdir(parents=True)
            (config / "profile.json").write_text(
                (root / "profiles/default.json").read_text()
            )
            (config / "desktop.json").write_text("{}\n")
            (config / "niri").mkdir(parents=True)
            (config / "niri/custom.kdl").write_text("// no volume overrides\n")
            state = home / ".local/state/nacre"
            state.mkdir(parents=True)
            (state / "scheme.json").write_text(
                '{"colours":{"primary":"#112233","secondary":"#445566",'
                '"outlineVariant":"#778899","error":"#aabbcc"}}\n'
            )

            shutil.rmtree(checkout)
            self.assertFalse(checkout.exists())
            environment = dict(os.environ)
            environment.update(
                HOME=str(home),
                XDG_CONFIG_HOME=str(home / ".config"),
                XDG_STATE_HOME=str(home / ".local/state"),
                XDG_CACHE_HOME=str(home / ".cache"),
            )
            for name in (
                "NIRI_SOCKET",
                "PYTHONPATH",
                "NACRE_HELPER_TOOLS",
                "NACRE_COMPOSITOR",
                "NACRE_COMPOSITOR_TOOLS",
            ):
                environment.pop(name, None)
            refreshed = subprocess.run(
                [str(home / ".local/bin/nacre"), "session", "refresh"],
                env=environment,
                capture_output=True,
                text=True,
            )
            self.assertEqual(
                refreshed.returncode, 0, refreshed.stdout + refreshed.stderr
            )
            self.assertIn("Niri session refreshed and validated", refreshed.stdout)

            generated = (session / "config/bindings.kdl").read_text()
            self.assertIn("XF86AudioRaiseVolume", generated)
            self.assertIn("XF86AudioLowerVolume", generated)
            self.assertIn("nacrectl key-step audio.step", generated)
            self.assertIn(
                "installed-revision", (session / "source-revision").read_text()
            )

    def test_session_refresh_validates_reloads_and_keeps_previous_tree(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder) / "home"
            session = home / ".local/share/nacre/niri-on-pc/session"
            session.mkdir(parents=True)
            (session / "source-revision").write_text("old\n")
            (session / "stock-niri").write_text("stock Niri\n")
            config = session / "config/config.kdl"
            config.parent.mkdir()
            config.write_text("previous config\n")
            helper = home / ".local/share/nacre/shell/tools/nacre-helper"
            helper.parent.mkdir(parents=True)
            helper.touch()
            calls = []

            def run(command, check):
                calls.append(command)
                if command[1] == "niri-session-runtime":
                    config.write_text("validated config\n")
                return subprocess.CompletedProcess(command, 0)

            with (
                patch.object(m, "HOME", home),
                patch.dict(os.environ, {"NIRI_SOCKET": "fixture.sock"}),
                patch.object(m.subprocess, "run", side_effect=run),
            ):
                m.refresh_session(Path("/fixture/nacre"))
            self.assertEqual(config.read_text(), "validated config\n")
            self.assertEqual(
                (
                    session.with_name("session.rollback") / "config/config.kdl"
                ).read_text(),
                "previous config\n",
            )
            self.assertEqual(
                [call[1:3] for call in calls],
                [
                    ["niri-session-runtime", "--backend"],
                    ["compositor", "action"],
                ],
            )
            self.assertEqual(
                calls[0][calls[0].index("--backend") + 1],
                str(home / ".local/share/nacre/shell/tools/backend"),
            )
            self.assertIn("--stock-niri", calls[0])

    def test_session_refresh_failure_restores_files_in_place(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder) / "home"
            session = home / ".local/share/nacre/niri-on-pc/session"
            session.mkdir(parents=True)
            (session / "source-revision").write_text("old\n")
            config = session / "config/config.kdl"
            config.parent.mkdir()
            config.write_text("previous config\n")
            inode = config.parent.stat().st_ino
            helper = home / ".local/share/nacre/shell/tools/nacre-helper"
            helper.parent.mkdir(parents=True)
            helper.touch()

            def fail(command, check):
                config.write_text("partial config\n")
                raise subprocess.CalledProcessError(1, command)

            with (
                patch.object(m, "HOME", home),
                patch.object(m.subprocess, "run", side_effect=fail),
            ):
                with self.assertRaises(subprocess.CalledProcessError):
                    m.refresh_session(Path("/fixture/nacre"))
            self.assertEqual(config.read_text(), "previous config\n")
            self.assertEqual(config.parent.stat().st_ino, inode)
            self.assertFalse(session.with_name("session.rollback").exists())

    def test_installed_controller_contains_session_modules_and_all_session_help(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            home = root / "home"
            control = home / ".local/share/nacre/control"
            tools = home / ".local/share/nacre/shell/tools"
            control.mkdir(parents=True)
            tools.mkdir(parents=True)
            for name in (
                "nacre_state.py",
                "nacre-state-files.json",
                "writer_lease.py",
            ):
                shutil.copy2(
                    Path(__file__).parents[2] / "nacre/shell-tools" / name, tools / name
                )
            with patch.multiple(m, HOME=home, CONTROL=control):
                m.install_controller(Path(__file__).parents[2])
            self.assertTrue((control / "package_sessions.py").is_file())
            self.assertTrue((control / "package_niri.py").is_file())
            wrapper = home / ".local/bin/nacre"
            for args in (
                ["--help"],
                ["session", "--help"],
                ["session", "niri", "--help"],
                ["session", "hyprland", "--help"],
                ["session", "refresh", "--help"],
            ):
                result = subprocess.run(
                    [str(wrapper), *args],
                    env={**os.environ, "HOME": str(home)},
                    capture_output=True,
                    text=True,
                )
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn("usage:", result.stdout)

    def test_deploy_contention_refuses_before_plan_or_destination_changes(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            home = root / "home"
            config = home / ".config/nacre"
            config.mkdir(parents=True)
            foreign = config / "profile.json"
            foreign.write_bytes(b"owner data")
            identity = foreign.stat().st_ino
            keyboard = root / "wtype"
            keyboard.touch()
            with (
                patch.object(m, "HOME", home),
                patch.object(m, "STATE", root / "state"),
                patch.object(m.subprocess, "check_output") as check_output,
                patch.object(m.subprocess, "run") as run,
                patch.object(m, "capture") as capture,
                m._writer.writer_lease(config),
            ):
                with self.assertRaises(m._writer.WriterLeaseError):
                    m.deploy(root, [], str(keyboard))
            check_output.assert_not_called()
            run.assert_not_called()
            capture.assert_not_called()
            self.assertEqual(foreign.read_bytes(), b"owner data")
            self.assertEqual(foreign.stat().st_ino, identity)

    def test_package_transaction_passes_held_lease_to_nested_writers(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            home = root / "home"
            home.mkdir()
            config = home / ".config/nacre"
            config.mkdir(parents=True)
            repo = root / "package"
            repo.mkdir()
            (repo / "package.json.installed").write_text("fixture")
            release = root / "release"
            record = {"entries": [], "created": "fixture", "previous": "baseline"}
            plan = Mock(return_value=[])
            install = Mock()
            profiles = SimpleNamespace(install=install)
            configure = SimpleNamespace(apply=Mock())
            package_runtime = SimpleNamespace(
                metadata=Mock(return_value={"revision": "fixture"}),
                apply=Mock(),
                legacy_units=Mock(),
            )
            setup = SimpleNamespace(
                supported_compositors=Mock(return_value=["mock"]),
                policy=Mock(return_value=profiles),
            )
            package_sessions = SimpleNamespace(backends=Mock(return_value=["mock"]))
            keyboard = SimpleNamespace(seed=Mock())
            with (
                patch.object(m, "HOME", home),
                patch.object(m, "STATE", root / "state"),
                patch.object(m, "package_plan", plan),
                patch.object(m, "package_configuration", return_value=nullcontext([])),
                patch.object(m, "capture", return_value=(release, record)),
                patch.object(m, "atomic"),
                patch.dict(
                    "sys.modules",
                    {
                        "configure": configure,
                        "package_runtime": package_runtime,
                        "setup": setup,
                        "package_sessions": package_sessions,
                        "keyboard": keyboard,
                    },
                ),
                m._writer.writer_lease(config) as lease,
            ):
                result = m._deploy_package_locked(
                    repo, {"id": "fixture"}, {"layout": "us"}, "mock", lease
                )
                self.assertEqual(result["status"], "prepared")
                self.assertIs(plan.call_args.kwargs["lease"], lease)
                self.assertIs(install.call_args.kwargs["lease"], lease)
                with self.assertRaises(m._writer.WriterLeaseError):
                    with m._writer.writer_lease(config):
                        self.fail("nested package writers released their parent lease")

    def test_capture_and_restore_config_directory_preserve_lock_inodes(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            home = root / "home"
            config = home / ".config/nacre"
            config.mkdir(parents=True)
            profile = config / "profile.json"
            profile.write_text("before")
            nested = config / "nested"
            nested.mkdir()
            nested_release_lock = nested / "release.lock"
            nested_release_lock.write_text("keep")
            with (
                patch.object(m, "HOME", home),
                patch.object(m, "STATE", root / "state"),
                m._writer.writer_lease(config) as lease,
            ):
                writer_identity = (config / ".writer.lock").stat().st_ino
                release, record = m.capture(
                    root, "fixture", [config], package=True, lease=lease
                )
                record["deployment"] = "package"
                backup = Path(record["entries"][0]["backup"])
                self.assertFalse((backup / ".writer.lock").exists())
                self.assertFalse((backup / "nested/release.lock").exists())
                profile.write_text("after")
                obsolete = config / "obsolete.json"
                obsolete.write_text("remove on restore")
                m.restore(release, record, force=True, lease=lease)
                self.assertEqual(profile.read_text(), "before")
                self.assertFalse(obsolete.exists())
                self.assertEqual(
                    (config / ".writer.lock").stat().st_ino, writer_identity
                )
                self.assertEqual(nested_release_lock.read_text(), "keep")

    def test_standalone_restore_locks_release_then_destination(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            home = root / "home"
            state = root / "state"

            def inspect_locked(release, record, force, *, lease):
                lease.ensure_valid()
                fd = os.open(state / "release.lock", os.O_RDWR)
                try:
                    with self.assertRaises(BlockingIOError):
                        fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
                finally:
                    os.close(fd)
                with self.assertRaises(m._writer.WriterLeaseError):
                    with m._writer.writer_lease(home / ".config/nacre"):
                        pass
                return "restored"

            with (
                patch.object(m, "HOME", home),
                patch.object(m, "STATE", state),
                patch.object(m, "_restore_locked", side_effect=inspect_locked),
            ):
                self.assertEqual(m.restore(root / "release", {}), "restored")
                with m._writer.writer_lease(home / ".config/nacre"):
                    pass
                fd = os.open(state / "release.lock", os.O_RDWR)
                try:
                    fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
                finally:
                    os.close(fd)

    def test_restore_with_parent_lease_does_not_reacquire_release_lock(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            home = root / "home"
            state = root / "state"
            state.mkdir()
            with (
                patch.object(m, "HOME", home),
                patch.object(m, "STATE", state),
                (state / "release.lock").open("a") as lock,
                m._writer.writer_lease(home / ".config/nacre") as lease,
            ):
                fcntl.flock(lock, fcntl.LOCK_EX)
                with patch.object(m, "_restore_locked", return_value="ok") as restore:
                    self.assertEqual(m.restore(root / "release", {}, lease=lease), "ok")
                    restore.assert_called_once()

    def test_capture_excludes_destination_and_transaction_lockfiles(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            home = root / "home"
            config = home / ".config/nacre"
            config.mkdir(parents=True)
            lease_file = config / ".writer.lock"
            transaction_lock = root / "state/release.lock"
            ordinary = root / "ordinary.json"
            lease_file.write_text("lease")
            lease_file.chmod(0o600)
            transaction_lock.parent.mkdir()
            transaction_lock.write_text("transaction")
            ordinary.write_text("ordinary")
            with (
                patch.object(m, "HOME", home),
                patch.object(m, "STATE", root / "state"),
                m._writer.writer_lease(config) as lease,
            ):
                release, record = m.capture(
                    root,
                    "fixture",
                    [lease_file, transaction_lock, ordinary],
                    package=True,
                    lease=lease,
                )
            self.assertEqual(
                [Path(item["path"]) for item in record["entries"]], [ordinary]
            )
            self.assertEqual(lease_file.read_text(), "lease")
            self.assertEqual(transaction_lock.read_text(), "transaction")

    def test_deploy_inherits_lease_fd_and_keeps_parent_lease_through_restore(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            home = root / "home"
            home.mkdir()
            keyboard = root / "wtype"
            keyboard.touch()
            release = root / "release"
            record = {"entries": [], "created": "fixture", "previous": "baseline"}
            outputs = iter(("revision\n", ""))

            def run(command, **kwargs):
                if "profile-policy.py" in " ".join(map(str, command)):
                    self.assertEqual(kwargs["pass_fds"], (lease.fd,))
                    self.assertEqual(
                        kwargs["env"]["NACRE_WRITER_LEASE_FD"], str(lease.fd)
                    )
                    with self.assertRaises(m._writer.WriterLeaseError):
                        with m._writer.writer_lease(home / ".config/nacre"):
                            pass
                    raise RuntimeError("stop after handoff check")
                return SimpleNamespace(returncode=1)

            with (
                patch.object(m, "HOME", home),
                patch.object(m, "STATE", root / "state"),
                patch.object(
                    m.subprocess,
                    "check_output",
                    side_effect=lambda *a, **k: next(outputs),
                ),
                patch.object(m.subprocess, "run", side_effect=run),
                patch.object(m, "capture", return_value=(release, record)),
                patch.object(m, "atomic"),
                patch.object(m, "restore") as restore,
                m._writer.writer_lease(home / ".config/nacre") as lease,
            ):
                with self.assertRaisesRegex(RuntimeError, "stop after handoff check"):
                    m._deploy_locked(root, [], str(keyboard), lease=lease)
                restore.assert_called_once()
                self.assertIs(restore.call_args.kwargs["lease"], lease)
                with self.assertRaises(m._writer.WriterLeaseError):
                    with m._writer.writer_lease(home / ".config/nacre"):
                        pass

    def test_doctor_forwards_export_mode_without_an_extra_json_prefix(self):
        import io
        import sys
        from contextlib import redirect_stdout

        with (
            tempfile.TemporaryDirectory() as folder,
            patch.object(m, "STATE", Path(folder)),
            patch.object(m.subprocess, "run") as run,
        ):
            for flag in ("--report", "--json"):
                with (
                    patch.object(sys, "argv", ["nacre", "doctor", flag]),
                    redirect_stdout(io.StringIO()) as output,
                ):
                    m.main()
                self.assertEqual(output.getvalue(), "")
                self.assertEqual(run.call_args.args[0][-2:], ["doctor", flag])
                self.assertTrue(run.call_args.kwargs["check"])

    def test_failed_release_restores_complete_software_and_preserves_account_state(
        self,
    ):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            repo = home / "repo"
            repo.mkdir()
            source = home / ".local/share/nacre/shell/source"
            source.mkdir(parents=True)
            (source / "shell.qml").write_text("before")
            account = home / ".codex/auth.json"
            account.parent.mkdir()
            account.write_text("fixture never snapshot")
            with (
                patch.object(m, "HOME", home),
                patch.object(m, "STATE", home / "releases"),
                patch.object(m.subprocess, "run"),
                patch.object(m, "compositor") as service,
            ):
                release, record = m.capture(repo, "new")
                (source / "shell.qml").write_text("broken")
                self.assertFalse(any(".codex" in e["path"] for e in record["entries"]))
                m.restore(release, record, force=True)
                service.return_value.reload.assert_called_once_with()
                self.assertEqual((source / "shell.qml").read_text(), "before")
                self.assertEqual(account.read_text(), "fixture never snapshot")

    def test_rollback_loads_backend_before_restoring_a_pre_facade_shell(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            repo = home / "repo"
            repo.mkdir()
            shell = home / ".local/share/nacre/shell"
            shell.mkdir(parents=True)
            (shell / "old.qml").write_text("old release")
            with (
                patch.object(m, "HOME", home),
                patch.object(m, "STATE", home / "releases"),
                patch.object(m.subprocess, "run"),
                patch.object(m, "compositor") as service,
            ):
                release, record = m.capture(repo, "new")
                facade = shell / "tools/nacre_native_compositor.py"
                facade.parent.mkdir()
                facade.write_text("new facade")
                desktop = service.return_value

                def load_before_restore():
                    self.assertTrue(facade.exists())
                    return desktop

                service.side_effect = load_before_restore
                m.restore(release, record, force=True)
                self.assertFalse(facade.exists())
                service.assert_called_once_with()
                desktop.reload.assert_called_once_with()

    def test_controller_is_part_of_the_snapshot(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            repo = home / "repo"
            repo.mkdir()
            with patch.object(m, "HOME", home):
                owned = m.paths(repo)
            self.assertIn(home / ".local/bin/nacre", owned)
            self.assertIn(home / ".local/share/nacre/control", owned)

    def test_rollback_refuses_later_edits(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            repo = home / "repo"
            repo.mkdir()
            source = home / ".local/bin/nacre-shell"
            source.parent.mkdir(parents=True)
            source.write_text("before")
            with (
                patch.object(m, "HOME", home),
                patch.object(m, "STATE", home / "releases"),
            ):
                release, record = m.capture(repo, "new")
                source.write_text("after")
                for e in record["entries"]:
                    e["after"] = m.fingerprint(Path(e["path"]))
                source.write_text("personal edit")
                with self.assertRaisesRegex(RuntimeError, "Later edit preserved"):
                    m.restore(release, record)
                self.assertEqual(source.read_text(), "personal edit")


class RuntimeRetirementTests(unittest.TestCase):
    def test_retirement_preserves_palette_environment_and_is_repeatable(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            base = home / ".local/share/nacre"
            native = base / "palette-runtime/usr/bin/quickshell"
            native.parent.mkdir(parents=True)
            native.write_text("old binary")
            palette = base / "palette-runtime/venv/keep.txt"
            palette.parent.mkdir()
            palette.write_text("palette environment")
            release = home / "release"
            with patch.object(m, "HOME", home):
                retired = m.retire_native_runtimes(release)
                self.assertEqual(len(retired), 1)
                self.assertFalse(native.exists())
                self.assertEqual(palette.read_text(), "palette environment")
                self.assertEqual(
                    Path(retired[0]["backup"]).joinpath("bin/quickshell").read_text(),
                    "old binary",
                )
                self.assertEqual(m.retire_native_runtimes(release), [])


class RetentionTests(unittest.TestCase):
    def test_newest_current_previous_and_retired_are_protected(self):
        import json

        with tempfile.TemporaryDirectory() as directory:
            state = Path(directory)
            for i in range(15):
                p = state / f"release-{i:02}"
                p.mkdir()
                (p / "release.json").write_text(
                    json.dumps({"status": "good", "revision": str(i), "previous": "1"})
                )
            (state / "release-00/retired-native").mkdir()
            (state / "current.json").write_text(
                json.dumps({"release": str(state / "release-02")})
            )
            with patch.object(m, "STATE", state):
                removed = m.prune_releases()
            self.assertEqual(
                {Path(p).name for p in removed}, {"release-03", "release-04"}
            )
            self.assertTrue((state / "release-00").exists())
            self.assertTrue((state / "release-01").exists())
            self.assertTrue((state / "release-02").exists())


class OptionalServiceGateTests(unittest.TestCase):
    def test_stopped_sidebar_is_not_required_or_started_by_desktop_deployment(self):
        active = {"nacre-shell.service", "nacre-brain.service"}
        self.assertNotIn(
            "nacre-sidebar-ai.service",
            m.required_live_services(["configs", "shell", "brain"], active),
        )
        self.assertIn(
            "nacre-sidebar-ai.service",
            m.required_live_services(["shell"], active | {"nacre-sidebar-ai.service"}),
        )
        self.assertIn(
            "nacre-brain.service",
            m.required_live_services(["brain"], set()),
        )


class OrientRuntimeSnapshotTests(unittest.TestCase):
    def test_palette_runtime_link_snapshots_bytes_and_restores_after_target_loss(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            repo = home / "repo"
            repo.mkdir()
            runtime = home / ".local/share/nacre/palette-runtime"
            target = home / "engine"
            (target / "venv").mkdir(parents=True)
            (target / "venv/keep").write_text("known-good-engine")
            runtime.parent.mkdir(parents=True)
            runtime.symlink_to(target)
            with (
                patch.object(m, "HOME", home),
                patch.object(m, "STATE", home / "releases"),
                patch.object(m.subprocess, "run"),
                patch.object(m, "compositor") as service,
            ):
                release, record = m.capture(repo, "orient")
                entry = next(e for e in record["entries"] if e["path"] == str(runtime))
                self.assertEqual(entry["kind"], "runtime")
                (target / "venv/keep").unlink()
                m.restore(release, record, force=True)
                service.return_value.reload.assert_called_once_with()
                self.assertEqual(
                    (runtime / "venv/keep").read_text(), "known-good-engine"
                )
