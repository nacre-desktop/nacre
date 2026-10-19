"""Production QML link logic on frozen transcripts, in a private offscreen UI."""

import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import tempfile
import threading
import time
import unittest

ROOT = Path(__file__).resolve().parents[2]


class PrivateDaemon:
    def __init__(self, path, scenario):
        self.path = path
        self.stop = threading.Event()
        self.active = []
        self.observed = {"calls": 0, "sessions": 0, "acks": 0}
        self.error = None
        self.listener = self.bind()
        self.thread = threading.Thread(target=self.run, args=(scenario,), daemon=True)
        self.thread.start()

    def bind(self):
        self.path.unlink(missing_ok=True)
        listener = socket.socket(socket.AF_UNIX)
        listener.bind(str(self.path))
        self.path.chmod(0o600)
        listener.listen()
        listener.settimeout(0.2)
        return listener

    def accept(self):
        until = time.monotonic() + 10
        while not self.stop.is_set() and time.monotonic() < until:
            try:
                stream, _ = self.listener.accept()
            except socket.timeout:
                continue
            stream.settimeout(5)
            self.active.append(stream)
            self.observed["sessions"] += 1
            return stream.makefile("rwb", buffering=0)
        raise TimeoutError("Private UI client did not connect")

    @staticmethod
    def read(stream):
        line = stream.readline(1_048_578)
        if not line:
            return None
        return json.loads(line)

    @staticmethod
    def send(stream, value):
        stream.write(json.dumps(value, separators=(",", ":")).encode() + b"\n")

    def welcome(self, stream, boot):
        if self.read(stream)["t"] != "hello":
            raise AssertionError("UI did not own hello")
        descriptor = {
            "v": 1,
            "class": "desktop",
            "timeout": 10000,
            "params": {"type": "object"},
            "result": {},
        }
        self.send(
            stream,
            {
                "t": "welcome",
                "proto": 1,
                "min": 1,
                "boot": boot,
                "topics": {"fixture": 1},
                "methods": {
                    name: descriptor
                    for name in ("fixture.hold", "fixture.echo", "ui.dispatch")
                },
            },
        )

    def run(self, scenario):
        try:
            scenario(self)
        except (
            AssertionError,
            OSError,
            ValueError,
            TypeError,
            KeyError,
            IndexError,
        ) as error:
            if not self.stop.is_set():
                self.error = repr(error)

    def close(self):
        self.stop.set()
        self.listener.close()
        for stream in self.active:
            try:
                stream.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass
            stream.close()
        self.thread.join(timeout=2)


@unittest.skipUnless(shutil.which("quickshell"), "Quickshell unavailable")
class DaemonClientTests(unittest.TestCase):
    def qml(
        self,
        body,
        *,
        asynchronous=False,
        wire=None,
        compositor=False,
        helpers=False,
        network=False,
        menu=False,
        settings=False,
        publisher=False,
        launcher=False,
        render_state=False,
        storage_client=False,
    ):
        helpers = helpers or network
        with tempfile.TemporaryDirectory(prefix="nacre-daemon-client-") as folder:
            root = Path(folder)
            services = root / "services"
            services.mkdir()
            for name in ("NacreDaemon.qml", "daemon-state.js"):
                shutil.copy2(ROOT / "nacre/shell/services" / name, services / name)
            # Translation is outside these private consumer/link tests.
            (services / "NacreI18n.qml").write_text(
                "pragma Singleton\nimport QtQuick\nQtObject { function tr(key, text, args) { return text; } }\n"
            )
            if helpers:
                for name in ("NacreHelpers.qml", "NacreHelperTask.qml"):
                    shutil.copy2(ROOT / "nacre/shell/services" / name, services / name)
            if network:
                shutil.copy2(ROOT / "nacre/shell/services/NacreNetwork.qml", services)
            if launcher:
                source = (
                    ROOT / "nacre/shell/services/LauncherPreferences.qml"
                ).read_text()
                (services / "LauncherPreferences.qml").write_text(
                    source.replace("NacreDaemon", "FixtureClient")
                )
                (services / "FixtureClient.qml").write_text("""pragma Singleton
import QtQuick
QtObject {
 property bool enabled:true
 property int loads:0
 property int subscribers:0
 property QtObject stream: QtObject {property var data:({});property bool fresh:false}
 function topic(name){return stream}
 function subscribe(name){subscribers++}
 function unsubscribe(name){subscribers--}
}""")
                (services / "NacreHelperTask.qml").write_text("""import QtQuick
QtObject {
 property var command:[]
 property bool running:false
 property bool stdinEnabled:false
 signal output(string line)
 signal started()
 signal exited()
 function write(value){}
 Component.onCompleted: if(running) FixtureClient.loads++
}""")
            if menu:
                shutil.copy2(ROOT / "nacre/shell/services/NacreMenuQuery.qml", services)
            if render_state:
                settings = True
                for folder in ("widgets", "config", "frame"):
                    (root / folder).mkdir()
                for name in ("NacreTokens.qml", "size-scale.json"):
                    shutil.copy2(
                        ROOT / "nacre/shell/widgets" / name, root / "widgets" / name
                    )
                shutil.copy2(
                    ROOT / "nacre/shell/config/NacreFrame.qml", root / "config"
                )
                shutil.copy2(
                    ROOT / "nacre/shell/modules/drawers/NacreFrameRim.qml",
                    root / "frame",
                )
                shutil.copy2(
                    ROOT / "nacre/shell-tools/tests/qml/fixtures/NacreColours.qml",
                    services,
                )
            if settings:
                for name in ("DesktopSettings.qml", "NacreSettingsStorage.qml"):
                    shutil.copy2(ROOT / "nacre/shell/services" / name, services)
                (services / "NacrePanelState.qml").write_text(
                    "pragma Singleton\nimport QtQuick\nQtObject {property bool settingsVisible:false}\n"
                )
                (services / "NacreCompositor.qml").write_text(
                    'pragma Singleton\nimport QtQuick\nQtObject {property string backendName:"hyprland";property var monitors:({values:[]})}\n'
                )
                tools = root / "home/.local/share/nacre/shell/tools"
                tools.mkdir(parents=True)
                shutil.copy2(ROOT / "nacre/shell-tools/nacre-state-files.json", tools)
            if storage_client:
                storage = services / "NacreSettingsStorage.qml"
                storage.write_text(
                    storage.read_text().replace("NacreDaemon", "FixtureClient")
                )
                (services / "FixtureClient.qml").write_text("""pragma Singleton
import QtQuick
QtObject {
 id:client
 property bool enabled:true
 property bool connected:true
 property string boot:"one"
 property var streams:({})
 property var calls:[]
 property var cancellations:[]
 signal newBoot(string id)
 property Component factory: Component {QtObject {property var data:({});property bool fresh:false;property int users:0}}
 function topic(name){if(!streams[name])streams[name]=factory.createObject(client);return streams[name]}
 function subscribe(name){topic(name).users++}
 function unsubscribe(name){topic(name).users--}
 function call(method,params,callback){calls=[...calls,{method:method,params:params,callback:callback}];return calls.length}
 function cancel(id){cancellations=[...cancellations,id]}
}""")
            if compositor:
                folder = services / "compositor"
                folder.mkdir()
                for name in ("NiriBackend.qml", "NiriWindow.qml", "NiriShortcut.qml"):
                    shutil.copy2(
                        ROOT / "nacre/shell/services/compositor" / name, folder / name
                    )
                (folder / "qmldir").write_text(
                    "singleton NiriBackend 1.0 NiriBackend.qml\nNiriWindow 1.0 NiriWindow.qml\nNiriShortcut 1.0 NiriShortcut.qml\n"
                )
            utils = root / "utils"
            utils.mkdir()
            shutil.copy2(ROOT / "nacre/shell/utils/NacrePaths.qml", utils)
            if publisher:
                shutil.copy2(ROOT / "nacre/shell/utils/NacreStateFile.qml", utils)
            runtime = root / "runtime"
            runtime.mkdir(mode=0o700)
            documents = {
                name: [
                    json.loads(line)
                    for line in (ROOT / "protocol/fixtures" / (name + ".jsonl"))
                    .read_text()
                    .splitlines()
                ]
                for name in ("state", "restart", "events")
            }
            source = """import QtQuick
import Quickshell
import qs.services
ShellRoot {
    id: root
    function check(value, message) { if (!value) throw new Error(message); }
    function finish(value) {
        console.log("DAEMON_CLIENT_RESULT " + JSON.stringify({ok:true, value:value || {}}));
        Qt.quit();
    }
    function run() {
        try {
            const link = NacreDaemon;
            const fixtures = FIXTURES;
            BODY
            FINISH
        } catch (error) {
            console.log("DAEMON_CLIENT_RESULT " + JSON.stringify({ok:false, error:String(error)}));
            Qt.quit();
        }
    }
    Component.onCompleted: Qt.callLater(run)
    Timer { interval:5000; running:true; onTriggered:Qt.quit() }
}
"""
            source = source.replace("FIXTURES", json.dumps(documents)).replace(
                "BODY", body
            )
            if publisher:
                source = source.replace(
                    "import qs.services", "import qs.services\nimport qs.utils"
                )
            if render_state:
                source = source.replace(
                    "import qs.services",
                    "import qs.services\nimport qs.config\nimport qs.widgets",
                )
            if compositor:
                source = source.replace(
                    "import qs.services",
                    "import qs.services\nimport qs.services.compositor",
                )
            source = source.replace("FINISH", "" if asynchronous else "finish();")
            if wire:
                source = source.replace("interval:5000", "interval:12000")
            (root / "shell.qml").write_text(source)
            environment = dict(os.environ)
            for key in (
                "DISPLAY",
                "WAYLAND_DISPLAY",
                "NIRI_SOCKET",
                "HYPRLAND_INSTANCE_SIGNATURE",
                "DBUS_SESSION_BUS_ADDRESS",
                "QML_IMPORT_PATH",
                "QML2_IMPORT_PATH",
                "QT_PLUGIN_PATH",
                "NACRE_DAEMON_SOCKET",
            ):
                environment.pop(key, None)
            environment.update(
                HOME=str(root / "home"),
                XDG_CONFIG_HOME=str(root / "config"),
                XDG_CACHE_HOME=str(root / "cache"),
                XDG_STATE_HOME=str(root / "state"),
                XDG_DATA_HOME=str(root / "data"),
                XDG_RUNTIME_DIR=str(runtime),
                QT_QPA_PLATFORM="offscreen",
                QT_LOGGING_RULES="*.debug=false\nqml.debug=true",
                NACRE_DAEMON_CLIENT="0",
                DBUS_SYSTEM_BUS_ADDRESS=f"unix:path={runtime}/absent-system-bus",
            )
            server = None
            if compositor:
                environment["NIRI_SOCKET"] = str(runtime / "niri.sock")
            if wire:
                helper = Path(
                    os.environ.get(
                        "NACRE_HELPER_BINARY", ROOT / "target/release/nacre-helper"
                    )
                )
                if not helper.is_file():
                    self.skipTest(
                        "Native helper unavailable; CI provides the candidate binary"
                    )
                installed = root / "home/.local/share/nacre/shell/tools/nacre-helper"
                installed.parent.mkdir(parents=True, exist_ok=True)
                installed.symlink_to(helper.resolve())
                server = PrivateDaemon(runtime / "daemon.sock", wire)
                environment.update(
                    NACRE_DAEMON_CLIENT="1", NACRE_DAEMON_SOCKET=str(server.path)
                )
            try:
                result = subprocess.run(
                    ["dbus-run-session", "--", "quickshell", "-p", str(root), "-n"],
                    env=environment,
                    text=True,
                    capture_output=True,
                    timeout=15 if wire else 8,
                    check=False,
                )
            finally:
                if server:
                    server.close()
            output = result.stdout + result.stderr
            lines = [
                line.split("DAEMON_CLIENT_RESULT ", 1)[1]
                for line in output.splitlines()
                if "DAEMON_CLIENT_RESULT " in line
            ]
            self.assertEqual(result.returncode, 0, output[-2000:])
            self.assertEqual(len(lines), 1, output[-2000:])
            value = json.loads(lines[0])
            self.assertTrue(value["ok"], value)
            if server:
                self.assertIsNone(server.error, server.error)
                self.assertFalse(
                    server.thread.is_alive(), "Private daemon fixture retained a worker"
                )
                return value["value"], server.observed
            return value["value"]

    def test_frozen_state_restart_and_events_keep_last_good_snapshot(self):
        self.qml("""
            const compositor = link.subscribe("compositor");
            for (const index of [0, 1, 2]) link.receive(JSON.stringify(fixtures.state[index]));
            check(compositor.sequence === 3 && compositor.data.windows.w1.focused, "initial coalesced state");
            const beforeGap = compositor.data;
            link.receive(JSON.stringify(fixtures.state[3]));
            check(!compositor.fresh && compositor.data === beforeGap, "gap must retain last good state");
            link.receive(JSON.stringify(fixtures.state[5]));
            check(compositor.fresh && compositor.sequence === 4 && !compositor.data.windows.w1.focused, "resync");
            for (const index of [0, 1]) link.receive(JSON.stringify(fixtures.restart[index]));
            const beforeRestart = compositor.data;
            link.receive(JSON.stringify(fixtures.restart[2]));
            check(!compositor.fresh && compositor.data === beforeRestart, "restart retains display");
            link.receive(JSON.stringify({t:"patch",topic:"compositor",base:20,seq:21,data:{invalid:true}}));
            check(compositor.data === beforeRestart, "old sequence ownership refused");
            link.receive(JSON.stringify(fixtures.restart[4]));
            check(compositor.fresh && compositor.sequence === 0 && !compositor.data.invalid, "new boot snapshot");
            let seen = [], lost = 0;
            link.event.connect((topic, name, data, seq) => seen.push(seq));
            link.lost.connect(topic => lost++);
            for (const message of fixtures.events) link.receive(JSON.stringify(message));
            check(JSON.stringify(seen) === "[1,6]" && lost === 1, "event ring loss");
        """)

    def test_merge_patch_shares_unchanged_branches_and_protects_object_prototype(self):
        self.qml("""
            const value = link.subscribe("compositor");
            link.receive(JSON.stringify(fixtures.state[0]));
            link.receive(JSON.stringify({t:"snap",topic:"compositor",seq:0,data:{windows:{w1:{id:"w1"}},order:["w1"]}}));
            const original = value.data;
            link.receive(JSON.stringify({t:"patch",topic:"compositor",base:0,seq:1,data:{focus:{id:"w1"}}}));
            check(value.data !== original && value.data.windows === original.windows && value.data.order === original.order, "unchanged branch identity");
            const unchanged = value.data;
            link.receive(JSON.stringify({t:"patch",topic:"compositor",base:1,seq:2,data:{focus:{id:"w1"}}}));
            check(value.data === unchanged, "no-op identity");
            link.receive('{"t":"patch","topic":"compositor","base":2,"seq":3,"data":{"__proto__":{"polluted":true},"windows":{"w1":null}}}');
            check(!({}).polluted && value.data.__proto__.polluted === true, "prototype key remains data");
            check(!value.data.windows.w1 && original.windows.w1.id === "w1", "delete without mutation");
        """)

    def test_independent_consumers_share_stateless_script_without_sharing_projections(
        self,
    ):
        self.qml(r"""
            const source='import QtQuick; import "services/daemon-state.js" as State; QtObject {readonly property var copier:State.copy;readonly property var merger:State.merge}';
            const first=Qt.createQmlObject(source,root,"first-consumer");
            const second=Qt.createQmlObject(source,root,"second-consumer");
            check(first.copier===second.copier&&first.merger===second.merger,"independent consumers retain one shared function context");
            const committed={unknown:{values:[1,2]}};
            const one=first.copier(committed),two=second.copier(committed);
            one.unknown.values.push(3);
            check(committed.unknown.values.length===2&&two.unknown.values.length===2,"sharing code never shares mutable projections");
        """)

    def test_last_subscriber_releases_cache_but_active_disconnect_keeps_state(self):
        self.qml("""
            const value = link.subscribe("fixture");
            link.subscribe("fixture");
            const snapshot = {items:{one:{id:"one"}},order:["one"]};
            value.data = snapshot;
            value.sequence = 17;
            value.fresh = true;
            link.unsubscribe("fixture");
            check(value.users === 1 && value.data === snapshot, "other view keeps its snapshot");
            link.disconnected();
            check(value.data === snapshot && !value.fresh, "active view keeps last good state offline");
            link.unsubscribe("fixture");
            check(value.users === 0 && Object.keys(value.data).length === 0 && value.sequence === 0, "last view releases cache offline");
            link.receive(JSON.stringify({t:"snap",topic:"fixture",seq:18,data:snapshot}));
            check(Object.keys(value.data).length === 0, "late state cannot repopulate a closed view");
            link.subscribe("fixture");
            check(value.users === 1 && !value.fresh && Object.keys(value.data).length === 0, "reopen needs a fresh snapshot");
        """)

    def test_closed_menu_releases_rows_and_reopens_with_a_fresh_query(self):
        self.qml(
            """
            const component = Qt.createComponent("services/NacreMenuQuery.qml");
            check(component.status === Component.Ready, component.errorString());
            const menu = component.createObject(root, {enabled:true,active:true,presented:true});
            menu.result = {order:["one"],items:{one:{name:"Fixture",activation:"opaque",presentation:{id:"one",operation:"copy",value:"fixture"}}}};
            menu.resultKey = menu.key;
            menu.resultBoot = link.boot;
            check(menu.entries.length === 1 && link.topic("menu").users === 1, "open menu owns rows and subscription");
            menu.active = false;
            check(menu.entries.length === 1 && link.topic("menu").users === 0, "closing animation retains rows without subscription");
            menu.presented = false;
            check(menu.entries.length === 0 && Object.keys(menu.result).length === 0, "closed menu releases rows");
            check(!menu.resultKey && !menu.resultBoot && !menu.dirty && link.topic("menu").users === 0, "closed menu releases query identity and subscription");
            menu.active = true;
            check(menu.dirty && menu.entries.length === 0 && link.topic("menu").users === 1, "reopen requests fresh rows");
        """,
            menu=True,
        )

    def test_settings_use_revisioned_calls_preserve_desired_values_and_scope_subscriptions(
        self,
    ):
        registry = json.loads(
            (ROOT / "nacre/shell-tools/nacre-state-files.json").read_text()
        )
        ids = [entry["id"] for entry in registry["files"] if entry.get("settings")]

        def scenario(server):
            with server.accept() as stream:
                self.assertEqual(server.read(stream)["t"], "hello")
                server.send(
                    stream,
                    {
                        "t": "welcome",
                        "proto": 1,
                        "min": 1,
                        "boot": "settings-test",
                        "topics": dict.fromkeys(
                            ["settings.desktop"]
                            + ["settings.store." + id for id in ids],
                            1,
                        ),
                        "methods": {
                            name: {
                                "v": 1,
                                "class": "desktop",
                                "timeout": 30000,
                                "params": {},
                                "result": {},
                            }
                            for name in [
                                "settings.get",
                                "settings.set",
                                "settings.desktop",
                            ]
                        },
                    },
                )
                desktop = {"dnd": False}
                revisions = dict.fromkeys(ids, 0)
                while not server.stop.is_set():
                    request = server.read(stream)
                    if request is None:
                        break
                    if request["t"] == "sub":
                        for topic in request["topics"]:
                            if not topic.startswith("settings.store."):
                                continue
                            store = topic.removeprefix("settings.store.")
                            server.send(
                                stream,
                                {
                                    "t": "snap",
                                    "topic": topic,
                                    "seq": 0,
                                    "data": {
                                        "store": store,
                                        "rev": revisions[store],
                                        "exists": True,
                                        "dirty": False,
                                        "data": desktop
                                        if store == "desktop"
                                        else {"enabled": revisions[store] > 0},
                                    },
                                },
                            )
                        continue
                    if request["t"] != "call":
                        continue
                    method, params = request["m"], request["p"]
                    if method == "settings.get":
                        server.observed["get_calls"] = (
                            server.observed.get("get_calls", 0) + 1
                        )
                    if method == "settings.desktop":
                        argv = params["arguments"]
                        if argv[0] == "set":
                            self.assertEqual(params["rev"], revisions["desktop"])
                            desktop[argv[1]] = json.loads(argv[2])
                            revisions["desktop"] += 1
                            server.observed["desktop_writes"] = (
                                server.observed.get("desktop_writes", 0) + 1
                            )
                        result = {
                            "available": True,
                            "data": desktop,
                            "monitors": [],
                            "pending": False,
                            "rev": revisions["desktop"],
                        }
                    else:
                        store = params["store"]
                        if method == "settings.set":
                            self.assertEqual(store, "power")
                            self.assertEqual(params["rev"], revisions[store])
                            self.assertEqual(params["key"], ["enabled"])
                            revisions[store] += 1
                            server.observed["power_writes"] = (
                                server.observed.get("power_writes", 0) + 1
                            )
                        result = {
                            "store": store,
                            "rev": revisions[store],
                            "exists": True,
                            "data": desktop
                            if store == "desktop"
                            else {"enabled": revisions[store] > 0},
                            "dirty": False,
                        }
                    server.send(stream, {"t": "ret", "id": request["id"], "ok": result})

        result, observed = self.qml(
            """
            const storage = NacreSettingsStorage, desktop = DesktopSettings;
            let phase = 0;
            const timer = Qt.createQmlObject('import QtQuick; Timer {interval:20;repeat:true}', root);
            timer.triggered.connect(() => {
                try {
                    if (phase === 0 && link.connected && storage.present.power && desktop.remoteRevision >= 0 && !desktop.busy) {
                        desktop.set("dnd", true);
                        check(desktop.data.dnd === true && desktop.busy, "desired value is immediate and serializes input writes");
                        storage.request("power", ["enabled"], true);
                        phase = 1;
                    } else if (phase === 1 && !desktop.busy && desktop.data.dnd === true && storage.documents.power?.enabled === true) {
                        check(Object.keys(desktop.desired).length === 0 && desktop.remoteRevision === 1, "ack clears only the matching desired revision");
                        NacrePanelState.settingsVisible = true;
                        phase = 2;
                    } else if (phase === 2 && link.topic("settings.store.colors").users === 1) {
                        NacrePanelState.settingsVisible = false;
                        phase = 3;
                    } else if (phase === 3 && link.topic("settings.store.colors").users === 0) {
                        check(link.topic("settings.store.power").users === 1, "power policy retains its live preference source");
                        timer.stop();finish({desktopRevision:desktop.remoteRevision,powerRevision:storage.revisions.power});
                    }
                } catch (error) {
                    console.log("DAEMON_CLIENT_RESULT " + JSON.stringify({ok:false,error:String(error)}));Qt.quit();
                }
            });
            timer.start();
        """,
            asynchronous=True,
            wire=scenario,
            settings=True,
        )
        self.assertEqual(result["desktopRevision"], 1)
        self.assertEqual(result["powerRevision"], 1)
        self.assertEqual(observed["desktop_writes"], 1)
        self.assertEqual(observed["power_writes"], 1)
        self.assertEqual(observed.get("get_calls", 0), 0)

    def test_disconnected_call_is_refused_before_dispatch(self):
        result = self.qml(
            """
            const id = link.call("ui.dispatch", {}, (value, failure) => finish({failure:failure, value:value}));
            check(id === 0 && link.pendingCount === 0, "offline calls cannot queue for replay");
        """,
            asynchronous=True,
        )
        self.assertEqual(result["failure"]["code"], "unavailable")
        self.assertFalse(result["failure"]["dispatched"])
        self.assertIsNone(result["value"])

    def test_real_bridge_caps_calls_acks_provider_and_resubscribes_without_replaying(
        self,
    ):
        def scenario(server):
            with server.accept() as first:
                server.welcome(first, "fixture-a")
                held = []
                while len(held) < 64:
                    message = server.read(first)
                    self.assertIsNotNone(message)
                    if message["t"] == "sub":
                        server.send(
                            first,
                            {
                                "t": "snap",
                                "topic": "fixture",
                                "seq": 0,
                                "data": {"phase": "a"},
                            },
                        )
                    elif message["t"] == "call":
                        self.assertEqual(message["m"], "fixture.hold")
                        held.append(message["id"])
                        server.observed["calls"] += 1
                server.send(
                    first,
                    {
                        "t": "call",
                        "id": "provider-1",
                        "m": "ui.dispatch",
                        "p": {},
                        "timeout": 1000,
                    },
                )
                reply = server.read(first)
                self.assertEqual(
                    reply, {"t": "ret", "id": "provider-1", "ok": {"accepted": True}}
                )
                server.observed["acks"] += 1
                for request in held[:-1]:
                    server.send(first, {"t": "ret", "id": request, "ok": {}})
                server.active[-1].shutdown(socket.SHUT_RDWR)
            with server.accept() as second:
                server.welcome(second, "fixture-b")
                while message := server.read(second):
                    if message["t"] == "sub":
                        server.send(
                            second,
                            {
                                "t": "snap",
                                "topic": "fixture",
                                "seq": 0,
                                "data": {"phase": "b"},
                            },
                        )
                    elif message["t"] == "call":
                        self.assertEqual(
                            message["m"],
                            "fixture.echo",
                            "A dispatched action was replayed",
                        )
                        server.send(
                            second,
                            {"t": "ret", "id": message["id"], "ok": {"ready": True}},
                        )

        result, observed = self.qml(
            """
            let stage = 0, succeeded = 0, failed = 0, busy = 0, loss = null;
            let oldData = null, retained = false;
            link.registerUi(root, (method, params) => ({ok:{accepted:true}}));
            const value = link.subscribe("fixture");
            link.newBoot.connect(boot => {
                if (boot === "fixture-b") retained = value.data === oldData && !value.fresh;
            });
            value.freshChanged.connect(() => {
                if (!value.fresh) return;
                if (stage === 0) {
                    stage = 1;
                    oldData = value.data;
                    for (let i = 0; i < 64; i++)
                        link.call("fixture.hold", {index:i}, (result, error) => {
                            if (error) { failed++; loss = error; } else succeeded++;
                        });
                    check(link.pendingCount === 64, "pending call bound");
                    const refused = link.call("fixture.hold", {}, (result, error) => { if (error.code === "busy" && error.dispatched === false) busy++; });
                    check(refused === 0, "65th call must not dispatch");
                } else if (stage === 1 && value.data.phase === "b") {
                    stage = 2;
                    link.call("fixture.echo", {}, (result, error) => finish({
                        succeeded:succeeded,failed:failed,busy:busy,loss:loss,retained:retained,
                        ready:result && result.ready,connected:link.connected,pending:link.pendingCount
                    }));
                }
            });
        """,
            asynchronous=True,
            wire=scenario,
        )
        self.assertEqual(observed, {"calls": 64, "sessions": 2, "acks": 1})
        self.assertEqual(
            (result["succeeded"], result["failed"], result["busy"]), (63, 1, 1)
        )
        self.assertEqual(result["loss"]["code"], "unavailable")
        self.assertNotEqual(result["loss"].get("dispatched"), False)
        self.assertTrue(result["retained"] and result["ready"] and result["connected"])
        self.assertEqual(result["pending"], 0)

    def test_reconnecting_mark_deadline_is_not_reset_by_failed_attempts(self):
        def scenario(server):
            with server.accept() as first:
                server.welcome(first, "fixture-a")
                while message := server.read(first):
                    if message["t"] == "sub":
                        server.send(
                            first,
                            {
                                "t": "snap",
                                "topic": "fixture",
                                "seq": 0,
                                "data": {"phase": "a"},
                            },
                        )
                        break
                server.listener.close()
                server.active[-1].shutdown(socket.SHUT_RDWR)
            if server.stop.wait(2.25):
                return
            server.listener = server.bind()
            with server.accept() as second:
                server.welcome(second, "fixture-b")
                while message := server.read(second):
                    if message["t"] == "sub":
                        server.send(
                            second,
                            {
                                "t": "snap",
                                "topic": "fixture",
                                "seq": 0,
                                "data": {"phase": "b"},
                            },
                        )

        result, observed = self.qml(
            """
            let disconnectedAt = 0, staleAfter = 0;
            const value = link.subscribe("fixture");
            link.connectedChanged.connect(() => { if (!link.connected) disconnectedAt = Date.now(); });
            link.staleChanged.connect(() => { if (link.stale && !staleAfter) staleAfter = Date.now() - disconnectedAt; });
            value.freshChanged.connect(() => {
                if (value.fresh && value.data.phase === "b")
                    finish({staleAfter:staleAfter,stale:link.stale,phase:value.data.phase});
            });
        """,
            asynchronous=True,
            wire=scenario,
        )
        self.assertEqual(observed["sessions"], 2)
        self.assertGreaterEqual(result["staleAfter"], 1900)
        self.assertFalse(result["stale"])
        self.assertEqual(result["phase"], "b")

    def test_niri_consumer_keeps_snapshot_rebinds_and_never_replays_an_action(self):
        def scenario(server):
            for generation in range(2):
                with server.accept() as stream:
                    self.assertEqual(server.read(stream)["t"], "hello")
                    descriptor = {
                        "v": 1,
                        "class": "desktop",
                        "timeout": 1000,
                        "params": {},
                        "result": {},
                    }
                    server.send(
                        stream,
                        {
                            "t": "welcome",
                            "proto": 1,
                            "min": 1,
                            "boot": f"niri-{generation}",
                            "topics": {"compositor": 1, "compositor.events": 1},
                            "methods": {
                                name: descriptor
                                for name in ("niri.source", "niri.action")
                            },
                        },
                    )
                    state = {
                        "backend": "niri",
                        "available": True,
                        "windows": {
                            "1": {
                                "windowId": "1",
                                "title": f"fixture-{generation}",
                                "workspaceId": 1,
                                "outputId": "fixture",
                            }
                        },
                        "windowOrder": ["1"],
                        "workspaces": {"1": {"id": 1, "index": 1}},
                        "workspaceOrder": ["1"],
                        "outputs": {"fixture": {"name": "fixture"}},
                        "outputOrder": ["fixture"],
                        "focus": {
                            "windowId": "1",
                            "workspaceId": "1",
                            "outputId": "fixture",
                        },
                    }
                    while message := server.read(stream):
                        if message["t"] == "sub":
                            for topic in message["topics"]:
                                server.send(
                                    stream,
                                    {
                                        "t": "snap",
                                        "topic": topic,
                                        "seq": 0,
                                        "data": state
                                        if topic == "compositor"
                                        else {
                                            "backend": "niri",
                                            "available": True,
                                            "connectionId": "1:2",
                                        },
                                    },
                                )
                        elif message.get("m") == "niri.source":
                            self.assertTrue(
                                message["p"]["socket"].endswith("/niri.sock")
                            )
                            server.observed["bindings"] = (
                                server.observed.get("bindings", 0) + 1
                            )
                            server.send(
                                stream,
                                {
                                    "t": "ret",
                                    "id": message["id"],
                                    "ok": {"connection_id": "1:2"},
                                },
                            )
                        elif message.get("m") == "niri.action":
                            self.assertEqual(
                                generation, 0, "Ambiguous action was replayed"
                            )
                            self.assertEqual(
                                message["p"],
                                {
                                    "connection_id": "1:2",
                                    "argv": [
                                        "action",
                                        "focus_direction",
                                        "--direction",
                                        "right",
                                    ],
                                },
                            )
                            server.observed["actions"] = (
                                server.observed.get("actions", 0) + 1
                            )
                            server.active[-1].shutdown(socket.SHUT_RDWR)
                            break

        result, observed = self.qml(
            """
            const backend = NiriBackend;
            let opened = false, kept = false;
            link.connectedChanged.connect(() => {
                if (!link.connected && opened)
                    kept = backend.byId["1"]?.title === "fixture-0";
            });
            backend.connectedChanged.connect(() => {
                if (!backend.connected) return;
                if (!opened) {
                    opened = true;
                    check(backend.activeClient.title === "fixture-0", "Initial compositor snapshot");
                    check(backend.action("focus_direction", ["--direction", "right"]), "Action accepted");
                } else {
                    Qt.callLater(() => finish({kept:kept,title:backend.activeClient.title,actions:backend.daemonActions}));
                }
            });
        """,
            asynchronous=True,
            wire=scenario,
            compositor=True,
        )
        self.assertEqual(observed["actions"], 1)
        self.assertEqual(observed["bindings"], 2)
        self.assertTrue(result["kept"])
        self.assertEqual(result["title"], "fixture-1")
        self.assertEqual(result["actions"], 0)

    def test_helper_consumer_keeps_large_packets_private_and_releases_them(self):
        def scenario(server):
            directory = server.path.parent / "lease"
            directory.mkdir(mode=0o700)
            with server.accept() as stream:
                self.assertEqual(server.read(stream)["t"], "hello")
                descriptor = {
                    "v": 1,
                    "class": "desktop",
                    "timeout": 45000,
                    "params": {},
                    "result": {},
                }
                server.send(
                    stream,
                    {
                        "t": "welcome",
                        "proto": 1,
                        "min": 1,
                        "boot": "helpers-a",
                        "topics": {},
                        "methods": {
                            name: descriptor
                            for name in ("helper.transfer", "helper.execute")
                        },
                    },
                )
                while message := server.read(stream):
                    if message.get("m") == "helper.transfer":
                        result = {"directory": str(directory)}
                        if message["p"].get("reserve"):
                            packet = directory / "packet-input"
                            packet.touch(mode=0o600)
                            result["file"] = packet.name
                        for name in message["p"].get("release", []):
                            (directory / name).unlink()
                            server.observed["released"] = (
                                server.observed.get("released", 0) + 1
                            )
                        server.send(
                            stream, {"t": "ret", "id": message["id"], "ok": result}
                        )
                    elif message.get("m") == "helper.execute":
                        if "file" in message["p"]:
                            packet = directory / message["p"]["file"]
                            self.assertEqual(packet.stat().st_mode & 0o777, 0o600)
                            request = json.loads(packet.read_text())
                            self.assertEqual(request["input"], ["é" * 600000])
                            response = directory / "packet-output"
                            response.touch(mode=0o600)
                            response.write_text(
                                json.dumps({"code": 0, "output": "é" * 600000})
                            )
                            result = {"file": response.name}
                        else:
                            self.assertEqual(
                                message["p"]["request"]["tool"], "notification-history"
                            )
                            result = {"code": 0, "output": "[]\n"}
                        server.observed["executed"] = (
                            server.observed.get("executed", 0) + 1
                        )
                        server.send(
                            stream, {"t": "ret", "id": message["id"], "ok": result}
                        )
            self.assertEqual(list(directory.iterdir()), [])

        result, observed = self.qml(
            """
            const helpers = NacreHelpers;
            let issued = false, completed = 0;
            function task(large) {
                return { command:["nacre-helper", "notification-history", large ? "save" : "load"],
                    complete:(code, status, text) => {
                        check(code === 0 && status === 0, "Helper completion");
                        check(text === (large ? "é".repeat(600000) : "[]\\n"), "Exact helper output");
                        completed++;
                    }
                };
            }
            function submit() {
                if (!helpers.ready || issued) return;
                issued = true;
                helpers.submit(task(false), null);
                helpers.submit(task(true), ["é".repeat(600000)]);
            }
            helpers.readyChanged.connect(submit);
            const timer = Qt.createQmlObject('import QtQuick; Timer {interval:20;repeat:true;running:true}', root);
            timer.triggered.connect(() => {
                if (completed === 2 && link.pendingCount === 0)
                    finish({completed:completed,pending:helpers.count});
            });
            submit();
        """,
            asynchronous=True,
            wire=scenario,
            helpers=True,
        )
        self.assertEqual(observed["executed"], 2)
        self.assertEqual(observed["released"], 2)
        self.assertEqual(result, {"completed": 2, "pending": 0})

    def test_network_consumer_uses_health_generation_and_debounces_refreshes(self):
        def scenario(server):
            with server.accept() as stream:
                self.assertEqual(server.read(stream)["t"], "hello")
                descriptor = {
                    "v": 1,
                    "class": "desktop",
                    "timeout": 45000,
                    "params": {},
                    "result": {},
                }
                server.send(
                    stream,
                    {
                        "t": "welcome",
                        "proto": 1,
                        "min": 1,
                        "boot": "network-a",
                        "topics": {"network.watch": 1},
                        "methods": {
                            name: descriptor
                            for name in ("helper.transfer", "helper.execute")
                        },
                    },
                )
                reads = 0
                while message := server.read(stream):
                    if message["t"] == "sub":
                        self.assertEqual(message["topics"], ["network.watch"])
                        server.send(
                            stream,
                            {
                                "t": "snap",
                                "topic": "network.watch",
                                "seq": 0,
                                "data": {"available": True, "generation": 0},
                            },
                        )
                    elif message.get("m") == "helper.transfer":
                        server.send(
                            stream,
                            {
                                "t": "ret",
                                "id": message["id"],
                                "ok": {"directory": str(server.path.parent)},
                            },
                        )
                    elif message.get("m") == "helper.execute":
                        self.assertEqual(
                            message["p"]["request"]["tool"], "network-state"
                        )
                        reads += 1
                        output = (
                            json.dumps(
                                {
                                    "wifiAvailable": True,
                                    "wifiEnabled": reads == 1,
                                    "wifiInterface": "fixture0",
                                    "networks": [],
                                }
                            )
                            + "\n"
                        )
                        server.send(
                            stream,
                            {
                                "t": "ret",
                                "id": message["id"],
                                "ok": {"code": 0, "output": output},
                            },
                        )
                        if reads == 1:
                            for revision in range(1, 6):
                                server.send(
                                    stream,
                                    {
                                        "t": "patch",
                                        "topic": "network.watch",
                                        "base": revision - 1,
                                        "seq": revision,
                                        "data": {"generation": revision},
                                    },
                                )
                        else:
                            self.assertEqual(
                                reads, 2, "Network changes were not debounced"
                            )
                    server.observed["reads"] = reads

        result, observed = self.qml(
            """
            const network = NacreNetwork;
            network.snapshotChanged.connect(() => {
                if (network.received && network.snapshot.wifiEnabled === false)
                    finish({reads:network.snapshotReads,generation:network.daemonWatch.data.generation,error:network.monitorError});
            });
            network.receivedChanged.connect(() => {
                if (network.received && network.snapshot.wifiEnabled === false)
                    finish({reads:network.snapshotReads,generation:network.daemonWatch.data.generation,error:network.monitorError});
            });
        """,
            asynchronous=True,
            wire=scenario,
            network=True,
        )
        self.assertEqual(observed["reads"], 2)
        self.assertEqual(result["generation"], 5)
        self.assertEqual(result["error"], "")

    def test_helper_consumer_fails_a_lost_mutation_once_without_replaying(self):
        def scenario(server):
            for generation in range(2):
                with server.accept() as stream:
                    self.assertEqual(server.read(stream)["t"], "hello")
                    descriptor = {
                        "v": 1,
                        "class": "desktop",
                        "timeout": 45000,
                        "params": {},
                        "result": {},
                    }
                    server.send(
                        stream,
                        {
                            "t": "welcome",
                            "proto": 1,
                            "min": 1,
                            "boot": f"helpers-{generation}",
                            "topics": {},
                            "methods": {
                                name: descriptor
                                for name in ("helper.transfer", "helper.execute")
                            },
                        },
                    )
                    while message := server.read(stream):
                        if message.get("m") == "helper.transfer":
                            server.send(
                                stream,
                                {
                                    "t": "ret",
                                    "id": message["id"],
                                    "ok": {"directory": str(server.path.parent)},
                                },
                            )
                        elif message.get("m") == "helper.execute":
                            self.assertEqual(
                                generation, 0, "Lost helper mutation was replayed"
                            )
                            server.observed["mutations"] = (
                                server.observed.get("mutations", 0) + 1
                            )
                            server.active[-1].shutdown(socket.SHUT_RDWR)
                            break

        result, observed = self.qml(
            """
            const helpers = NacreHelpers;
            let issued = false, completions = 0;
            helpers.readyChanged.connect(() => {
                if (!helpers.ready) return;
                if (!issued) {
                    issued = true;
                    helpers.submit({command:["nacre-helper","notification-history","save"],complete:(code, status, text) => {
                        check(code === 1, "Lost mutation must fail");
                        completions++;
                    }}, []);
                } else {
                    Qt.callLater(() => finish({completions:completions,pending:helpers.count}));
                }
            });
        """,
            asynchronous=True,
            wire=scenario,
            helpers=True,
        )
        self.assertEqual(observed["mutations"], 1)
        self.assertEqual(observed["sessions"], 2)
        self.assertEqual(result, {"completions": 1, "pending": 0})

    def test_ui_dispatch_uses_registered_handlers_and_rejects_method_injection(self):
        result = self.qml(
            """
            const handler = Qt.createQmlObject('import QtQuick; import Quickshell.Io; IpcHandler {target:"privateFixture";property int calls:0;function step(value:int):int {calls++;return value + 1;} }', root);
            check(link.registerIpc("fixture", handler, {step:["i"]}), "Register handler");
            const valid = link.dispatchIpc({target:"fixture",method:"step",args:["41"]});
            check(valid.ok === 42 && handler.calls === 1, "Existing handler invoked");
            for (const args of [["NaN"], ["1+1"], ["1.5"], ["2147483648"], [], [41]])
                check(!!link.dispatchIpc({target:"fixture",method:"step",args:args}).err, "Typed argument refusal");
            for (const method of ["constructor", "toString", "destroy", "step;false"])
                check(!!link.dispatchIpc({target:"fixture",method:method,args:["1"]}).err, "Method allowlist");
            check(!!link.dispatchIpc({target:"__proto__",method:"step",args:["1"]}).err, "Target allowlist");
            check(handler.calls === 1, "Rejected calls did not execute");
            link.unregisterIpc("fixture", handler);
            check(!!link.dispatchIpc({target:"fixture",method:"step",args:["1"]}).err, "Unregister handler");
            finish({calls:handler.calls});
        """,
            asynchronous=True,
        )
        self.assertEqual(result["calls"], 1)

    def test_menu_consumer_coalesces_queries_discards_obsolete_results_and_uses_tickets(
        self,
    ):
        def scenario(server):
            with server.accept() as stream:
                self.assertEqual(server.read(stream)["t"], "hello")
                descriptor = {
                    "v": 1,
                    "class": "desktop",
                    "timeout": 45000,
                    "params": {},
                    "result": {},
                }
                server.send(
                    stream,
                    {
                        "t": "welcome",
                        "proto": 1,
                        "min": 1,
                        "boot": "menu-a",
                        "topics": {"menu": 1},
                        "methods": {
                            name: descriptor for name in ("menu.query", "menu.activate")
                        },
                    },
                )
                queries = []
                while message := server.read(stream):
                    if message["t"] == "sub":
                        server.send(
                            stream,
                            {
                                "t": "snap",
                                "topic": "menu",
                                "seq": 0,
                                "data": {"available": True, "catalog_rev": 3},
                            },
                        )
                    elif message.get("m") == "menu.query":
                        queries.append(message["p"]["query"])
                        self.assertEqual(message["p"]["filter"], "files")
                        self.assertEqual(message["p"]["category"], "fixture")
                        self.assertIs(message["p"]["presentation"], True)
                        row = {
                            "id": "copy",
                            "name": "Display fixture",
                            "kind": "action",
                            "operation": "copy",
                            "value": "literal",
                            "daemonTicket": "forged",
                        }
                        result = {
                            "query": queries[-1],
                            "mode": "apps",
                            "route": "home",
                            "catalog_rev": 3,
                            "order": ["nacre-menu:copy"],
                            "items": {
                                "nacre-menu:copy": {
                                    "name": "Display fixture",
                                    "kind": "action",
                                    "disabled": False,
                                    "activation": "opaque-" + queries[-1],
                                    "presentation": row,
                                }
                            },
                        }
                        server.send(
                            stream, {"t": "ret", "id": message["id"], "ok": result}
                        )
                    elif message.get("m") == "menu.activate":
                        self.assertEqual(
                            message["p"],
                            {
                                "id": "nacre-menu:copy",
                                "activation": "opaque-latest",
                                "catalog_rev": 3,
                            },
                        )
                        self.assertEqual(queries, ["first", "latest"])
                        server.observed["queries"] = queries
                        server.send(
                            stream,
                            {
                                "t": "ret",
                                "id": message["id"],
                                "ok": {"keep_open": True},
                            },
                        )

        result, observed = self.qml(
            """
            const component = Qt.createComponent("services/NacreMenuQuery.qml");
            check(component.status === Component.Ready, component.errorString());
            const menu = component.createObject(root, {query:"first",resultFilter:"files",category:"fixture"});
            let switched = false, shown = [];
            menu.pendingChanged.connect(() => {
                if (menu.pending && !switched) {
                    switched = true;
                    menu.query = "second";
                    menu.query = "latest";
                }
            });
            menu.resultChanged.connect(() => shown.push(menu.result.query));
            menu.currentChanged.connect(() => {
                if (!menu.current) return;
                const entry = menu.entries[0];
                check(entry.operation === "copy" && entry.value === "literal", "Presentation retained");
                check(entry.daemonTicket === "opaque-latest", "Ticket overrides metadata");
                menu.activate(entry, (value, failure) => {
                    check(!failure && value.keep_open, "Activation reply");
                    finish({shown:shown});
                });
            });
        """,
            asynchronous=True,
            wire=scenario,
            menu=True,
        )
        self.assertEqual(observed["queries"], ["first", "latest"])
        self.assertEqual(result["shown"], ["latest"])

    def test_publisher_waits_for_durable_snapshot_and_uses_no_file_process(self):
        def scenario(server):
            stream = server.accept()
            self.assertEqual(server.read(stream)["t"], "hello")
            descriptor = {
                "v": 1,
                "class": "desktop",
                "timeout": 5000,
                "params": {},
                "result": {},
            }
            server.send(
                stream,
                {
                    "t": "welcome",
                    "proto": 1,
                    "min": 1,
                    "boot": "publish-a",
                    "topics": {"settings.store.control-center": 1},
                    "methods": {
                        "settings.get": descriptor,
                        "settings.publish": descriptor,
                    },
                },
            )
            snapshot = {
                "store": "control-center",
                "rev": 0,
                "exists": False,
                "data": {"version": 2, "tiles": ["volume"]},
                "dirty": False,
            }
            calls = 0
            while message := server.read(stream):
                if message["t"] == "sub":
                    server.send(
                        stream,
                        {
                            "t": "snap",
                            "topic": "settings.store.control-center",
                            "seq": 0,
                            "data": snapshot,
                        },
                    )
                elif message.get("m") == "settings.get":
                    server.send(
                        stream, {"t": "ret", "id": message["id"], "ok": snapshot}
                    )
                elif message.get("m") == "settings.publish":
                    calls += 1
                    self.assertEqual(
                        message["p"],
                        {
                            "store": "control-center",
                            "rev": 0,
                            "value": {"version": 2, "tiles": ["vpn"]},
                        },
                    )
                    snapshot = dict(
                        snapshot, rev=1, data=message["p"]["value"], dirty=True
                    )
                    server.send(
                        stream, {"t": "ret", "id": message["id"], "ok": snapshot}
                    )
                    # The component must not emit saved on the in-memory ACK.
                    time.sleep(0.2)
                    server.send(
                        stream,
                        {
                            "t": "patch",
                            "topic": "settings.store.control-center",
                            "base": 0,
                            "seq": 1,
                            "data": {
                                "rev": 1,
                                "data": {"tiles": ["vpn"]},
                                "dirty": True,
                            },
                        },
                    )
                    time.sleep(0.1)
                    server.send(
                        stream,
                        {
                            "t": "patch",
                            "topic": "settings.store.control-center",
                            "base": 1,
                            "seq": 2,
                            "data": {"exists": True, "dirty": False},
                        },
                    )
                server.observed["publishes"] = calls

        result, observed = self.qml(
            r"""
            const store=Qt.createQmlObject('import QtQuick; import qs.utils; import qs.services; NacreStateFile {daemonClient:NacreDaemon; storeId:"control-center"; sourcePath:"/never-write-here"}',root);
            let received=false;
            store.stateReceived.connect((value,missing)=>{
                if (!received) {received=true;store.saveText(JSON.stringify({version:2,tiles:["vpn"]}));}
            });
            store.saved.connect(()=>finish({revision:store.revision,busy:store.daemonBusy,path:store.path,error:store.stateError}));
            """,
            asynchronous=True,
            wire=scenario,
            publisher=True,
        )
        self.assertEqual(observed["publishes"], 1)
        self.assertEqual(result["revision"], 1)
        self.assertFalse(result["busy"])
        self.assertEqual(result["path"], "")
        self.assertEqual(result["error"], "")

    def test_launcher_snapshot_initial_late_reconnect_and_boot_preserve_seeded_favorites(
        self,
    ):
        self.qml(
            r"""
            const client=FixtureClient;
            const favorites=Array.from({length:12},(_,i)=>"performance-demo-"+String(i).padStart(2,"0"));
            const snapshot={store:"launcher",rev:0,exists:true,dirty:false,data:{favorites:favorites,usage:{one:{count:2,last:10}}}};
            // A late consumer must project the already committed snapshot.
            client.stream.data=snapshot;client.stream.fresh=true;
            const preferences=LauncherPreferences;
            check(preferences.ready&&preferences.favorites.length===12,"late subscription retains all favorites");
            check(client.loads===0&&client.subscribers===1,"native initialization has one source and no legacy read");
            preferences.favorites.push("local-only");preferences.usage.one.count=99;
            check(snapshot.data.favorites.length===12&&snapshot.data.usage.one.count===2,"mutable projection cannot alter committed topic");
            for(const rev of [1,0]) {
                client.stream.fresh=false;
                client.stream.data={store:"launcher",rev:rev,exists:true,dirty:false,data:{favorites:favorites}};
                client.stream.fresh=true;
                check(preferences.favorites.length===12&&preferences.ready,"reconnect/new boot retains equivalent seeded count");
            }
            client.stream.data={store:"launcher",rev:1,exists:true,dirty:false,data:{favorites:favorites.slice(0,11)}};
            check(preferences.favorites.length===11,"live update remains reactive");
        """,
            launcher=True,
        )

    def test_launcher_offline_initialization_keeps_legacy_text_reader(self):
        self.qml(
            r"""
            FixtureClient.enabled=false;
            const preferences=LauncherPreferences;
            check(FixtureClient.loads===1&&FixtureClient.subscribers===0,"offline mode retains its legacy reader");
            preferences.accept('{"favorites":["one","two"],"hidden":[],"recent":["two"],"usage":{}}');
            check(preferences.favorites.length===2&&preferences.recent[0]==="two"&&!preferences.error,"legacy text projection remains compatible");
        """,
            launcher=True,
        )

    def test_desktop_snapshot_and_reply_share_one_owned_projection(self):
        self.qml(
            r"""
            const desktop=DesktopSettings;
            let projections=0;desktop.persistedDataChanged.connect(()=>projections++);
            const snapshot={data:{dnd:false,unknown:{keep:[1,2]}},monitors:[{name:"one",modes:["mode"]}],pending:false};
            desktop.acceptData(snapshot);desktop.acceptData(JSON.parse(JSON.stringify(snapshot)));
            check(projections===1,"equivalent snapshot and reply project settings once");
            desktop.persistedData.unknown.keep.push(3);desktop.monitors[0].modes.push("local");
            check(snapshot.data.unknown.keep.length===2&&snapshot.monitors[0].modes.length===1,"mutable desktop projection cannot alter committed snapshot");
            desktop.accept('{"data":{"dnd":true,"unknown":{"kept":true}},"pending":false}');
            check(desktop.data.dnd&&desktop.data.unknown.kept,"legacy text response preserves complete data");
        """,
            settings=True,
        )

    def test_sparse_file_and_normalized_snapshot_have_identical_frame_and_token_state(
        self,
    ):
        defaults = json.loads(
            (ROOT / "nacre/shell-tools/desktop-defaults.json").read_text()
        )
        self.qml(
            r"""
            const desktop=DesktopSettings;
            const rimComponent=Qt.createComponent("frame/NacreFrameRim.qml");
            check(rimComponent.status===Component.Ready,rimComponent.errorString());
            const rim=rimComponent.createObject(root,{contour:"M0,0 L100,0 L100,100 Z"});
            function observed() {
                return JSON.stringify({frame:[NacreFrame.thickness,NacreFrame.headerHeight,NacreFrame.left,NacreFrame.right,NacreFrame.bottom,NacreFrame.rounding,String(NacreFrame.colour)],text:[NacreTokens.textFamily,NacreTokens.monoFamily,NacreTokens.textSize,NacreTokens.textPointSize],motion:NacreTokens.motionEnabled,gradients:NacreTokens.gradientsEnabled,colours:[String(NacreTokens.body),String(NacreTokens.raised),String(NacreTokens.ink),String(NacreTokens.accent)],rim:[rim.enabled,rim.opacity]});
            }
            desktop.persistedData={language:"en"};
            const sparse=observed();
            desktop.accept(JSON.stringify({data:Object.assign({},DEFAULTS,{language:"en"}),monitors:[],pending:false}));
            check(observed()===sparse,"normalized native snapshot preserves sparse-file frame geometry, text, colours, rim opacity and effects");
            check(NacreFrame.headerHeight===50&&NacreFrame.rounding===25&&NacreTokens.textPointSize===12,"seeded defaults resolve to expected visible geometry");
            finish({effective:JSON.parse(observed())});
        """.replace("DEFAULTS", json.dumps(defaults)),
            render_state=True,
            asynchronous=True,
        )

    def test_storage_subscription_lifecycle_uses_snapshots_without_implicit_reads(self):
        self.qml(
            r"""
            const client=FixtureClient;
            const power=client.topic("settings.store.power");
            power.data={store:"power",rev:3,exists:true,data:{enabled:true,unknown:{keep:1}},dirty:false};power.fresh=true;
            const storage=NacreSettingsStorage;
            check(client.calls.length===0&&power.users===1,"initial subscription needs no parallel read");
            check(storage.documents.power.unknown.keep===1&&storage.revisions.power===3,"late subscriber projects current cached document");
            client.connected=false;power.fresh=false;
            client.connected=true;
            check(storage.revisions.power===undefined&&client.calls.length===0,"reconnect does not consume retained stale revision or request read");
            power.data={store:"power",rev:4,exists:true,data:{enabled:false},dirty:false};power.fresh=true;
            check(storage.revisions.power===4&&!storage.documents.power.enabled,"fresh reconnect snapshot restores state");
            power.fresh=false;client.boot="two";client.newBoot("two");
            check(storage.revisions.power===undefined&&client.calls.length===0,"new boot waits for full snapshot");
            power.data={store:"power",rev:0,exists:true,data:{enabled:true},dirty:false};power.fresh=true;
            check(storage.revisions.power===0&&storage.documents.power.enabled,"new boot accepts its own revision namespace");
            const colors=client.topic("settings.store.colors");
            colors.data={store:"colors",rev:9,exists:true,data:{unknown:"old boot"},dirty:false};colors.fresh=false;
            NacrePanelState.settingsVisible=true;
            check(colors.users===1&&storage.documents.colors===undefined&&client.calls.length===0,"adding watched store refuses cached stale boot data without a parallel read");
            colors.data={store:"colors",rev:0,exists:true,data:{unknown:"preserved"},dirty:false};colors.fresh=true;
            check(storage.documents.colors.unknown==="preserved"&&storage.revisions.colors===0,"added store accepts its current subscribed snapshot");
            NacrePanelState.settingsVisible=false;
            check(colors.users===0&&power.users===1,"closing settings releases only optional subscriptions");
        """,
            settings=True,
            storage_client=True,
        )

    def test_storage_old_boot_read_callback_cannot_replace_revision_zero_or_new_request(
        self,
    ):
        self.qml(
            r"""
            const client=FixtureClient,storage=NacreSettingsStorage;
            const power=client.topic("settings.store.power");
            power.data={store:"power",rev:6,exists:true,data:{enabled:false},dirty:false};power.fresh=true;
            storage.request("power");const old=client.calls[0].callback;
            power.fresh=false;client.boot="two";client.newBoot("two");
            const disconnectedError=storage.errors.power;
            old({store:"power",rev:7,exists:true,data:{enabled:false},dirty:false},null);
            check(storage.revisions.power===undefined&&storage.errors.power===disconnectedError&&!storage.daemonBusy,"old boot read changes no current state or error");
            power.data={store:"power",rev:0,exists:true,data:{enabled:true},dirty:false};power.fresh=true;
            check(storage.revisions.power===0&&storage.documents.power.enabled&&storage.errors.power==="","fresh lower revision remains admissible");
            storage.request("power");const current=client.calls[1].callback;
            old(null,{message:"stale failure"});
            check(storage.daemonBusy&&storage.errors.power===""&&storage.revisions.power===0,"late callback cannot release new request or replace its metadata");
            current({store:"power",rev:0,exists:true,data:{enabled:true},dirty:false},null);
            check(!storage.daemonBusy&&storage.queue.length===0,"only current request owns completion");
        """,
            settings=True,
            storage_client=True,
        )

    def test_storage_old_boot_mutation_callback_never_replays_or_completes_new_write(
        self,
    ):
        self.qml(
            r"""
            const client=FixtureClient,storage=NacreSettingsStorage;
            const power=client.topic("settings.store.power");
            power.data={store:"power",rev:6,exists:true,data:{enabled:false},dirty:false};power.fresh=true;
            storage.request("power",["enabled"],true);const old=client.calls[0].callback;
            check(client.calls[0].method==="settings.set"&&storage.queue.length===0,"first mutation dispatched once");
            power.fresh=false;client.boot="two";client.newBoot("two");
            power.data={store:"power",rev:0,exists:true,data:{enabled:true},dirty:false};power.fresh=true;
            old({store:"power",rev:7,exists:true,data:{enabled:false},dirty:true},null);
            check(storage.revisions.power===0&&storage.documents.power.enabled&&client.calls.length===1,"late dispatched reply cannot replace new state or replay write");
            storage.request("power",["enabled"],false);const current=client.calls[1].callback;
            old(null,{message:"old failure"});
            check(storage.daemonBusy&&storage.errors.power===""&&client.calls[1].params.rev===0,"old reply cannot complete or fail the new write");
            current({store:"power",rev:1,exists:true,data:{enabled:false},dirty:false},null);
            check(storage.revisions.power===1&&!storage.documents.power.enabled&&!storage.daemonBusy,"new write owns its own completion");
        """,
            settings=True,
            storage_client=True,
        )

    def test_storage_old_revision_read_cannot_dispatch_its_intent_or_remove_new_queue_owner(
        self,
    ):
        self.qml(
            r"""
            const client=FixtureClient,storage=NacreSettingsStorage;
            storage.request("dashboard",["lyrics"],false);const old=client.calls[0].callback;
            storage.request("power",["enabled"],true);
            client.boot="two";client.newBoot("two");
            check(client.calls.length===2&&client.calls[1].params.store==="power"&&storage.queue.length===1,"new queued intent survives old revision-read invalidation");
            old({store:"dashboard",rev:7,exists:true,data:{lyrics:true},dirty:false},null);
            check(client.calls.length===2&&storage.queue.length===1&&storage.queue[0].id==="power"&&storage.revisions.dashboard===undefined,"old callback cannot populate stale revision or own the new queue item");
            client.calls[1].callback({store:"power",rev:0,exists:true,data:{enabled:false},dirty:false},null);
            Qt.callLater(()=>{
                check(client.calls.length===3&&client.calls[2].method==="settings.set"&&client.calls[2].params.store==="power"&&client.calls[2].params.rev===0,"only the new queued intent dispatches with current revision");
                old(null,{message:"old read failure"});
                check(storage.daemonBusy&&storage.queue.length===0,"old read cannot release current mutation ownership");
                client.calls[2].callback({store:"power",rev:1,exists:true,data:{enabled:true},dirty:false},null);
                finish();
            });
        """,
            settings=True,
            storage_client=True,
            asynchronous=True,
        )

    def test_storage_unwatched_mutation_reads_revision_once_without_replay(self):
        self.qml(
            r"""
            const client=FixtureClient,storage=NacreSettingsStorage;
            storage.request("dashboard",["lyrics"],false);
            check(client.calls.length===1&&client.calls[0].method==="settings.get","unwatched mutation acquires its revision exactly once");
            client.calls[0].callback({store:"dashboard",rev:7,exists:true,data:{lyrics:true},dirty:false},null);
            Qt.callLater(()=>{
                check(client.calls.length===2&&client.calls[1].method==="settings.set"&&client.calls[1].params.rev===7,"queued mutation uses acquired revision");
                client.calls[1].callback(null,{message:"connection lost after dispatch"});
                client.connected=false;client.connected=true;client.boot="two";client.newBoot("two");
                check(client.calls.length===2&&storage.queue.length===0&&storage.errors.dashboard==="connection lost after dispatch","reconnect and new boot never replay a dispatched mutation");
                finish();
            });
        """,
            settings=True,
            storage_client=True,
            asynchronous=True,
        )

    def test_storage_equal_documents_preserve_metadata_explicit_reads_and_mutation_revision(
        self,
    ):
        self.qml(
            r"""
            const client=FixtureClient,storage=NacreSettingsStorage;
            const power=client.topic("settings.store.power");
            let projections=0;storage.documentsChanged.connect(()=>projections++);
            const document={enabled:true,unknown:{keep:1}};
            power.data={store:"power",rev:2,exists:true,data:document,dirty:false};power.fresh=true;
            const projected=storage.documents;
            power.data={store:"power",rev:3,exists:false,data:{unknown:{keep:1},enabled:true},dirty:true,error:{message:"cannot persist"}};
            check(storage.documents===projected&&projections===1,"equal document avoids rebuilding dependent models");
            check(storage.revisions.power===3&&!storage.present.power&&storage.errors.power==="cannot persist","revision, existence and publication error remain reactive with equal data");
            power.data={store:"power",rev:3,exists:true,data:document,dirty:false};
            check(storage.present.power&&storage.errors.power===""&&projections===1,"clean error-free metadata clears failure independently of data");
            storage.documents.power.unknown.keep=2;
            check(document.unknown.keep===1,"mutable settings UI document cannot alter the shared committed topic");
            check(storage.request("power")&&client.calls.length===1&&client.calls[0].method==="settings.get","explicit read remains one RPC");
            power.data={store:"power",rev:4,exists:true,data:{enabled:false},dirty:false};
            client.calls[0].callback({store:"power",rev:3,exists:true,data:document,dirty:false},null);
            check(storage.revisions.power===4&&!storage.documents.power.enabled,"older explicit reply cannot replace newer subscribed state");
            storage.request("power",["enabled"],true);
            check(client.calls.length===2&&client.calls[1].method==="settings.set"&&client.calls[1].params.rev===4,"mutation uses current subscribed revision without extra read");
            client.calls[1].callback({store:"power",rev:5,exists:true,data:{enabled:true},dirty:true,error:{message:"not durable"}},null);
            check(storage.revisions.power===5&&storage.errors.power==="not durable","dirty mutation failure is visible and never invented as durable success");
        """,
            settings=True,
            storage_client=True,
        )

    def test_publisher_projects_snapshot_and_reply_once_and_ignores_older_read(self):
        self.qml(
            r"""
            const topic=Qt.createQmlObject('import QtQuick; QtObject {property var data:null}',root);
            const client=Qt.createQmlObject('import QtQuick; QtObject {property bool enabled:true;property bool connected:true;property string boot:"b1";property var topicObject:null;property var calls:[];signal newBoot(string id);function topic(name){return topicObject;}function subscribe(name){}function unsubscribe(name){}function call(method,params,callback){calls.push({method:method,callback:callback});return calls.length;}}',root);
            client.topicObject=topic;
            const component=Qt.createComponent("utils/NacreStateFile.qml");
            check(component.status===Component.Ready,component.errorString());
            const store=component.createObject(root,{daemonClient:client,storeId:"control-center"});
            let projections=0,last=null;store.stateReceived.connect(value=>{projections++;last=JSON.parse(value)});
            const initial={store:"control-center",rev:0,exists:true,dirty:false,data:{version:2,tiles:["vpn"],unknown:{keep:true}}};
            topic.data=initial;client.calls[0].callback(initial,null);
            check(projections===1&&last.unknown.keep,"snapshot plus get reply projects once with unknown fields");
            topic.data={store:"control-center",rev:1,exists:true,dirty:false,data:{version:2,tiles:["battery"]}};
            store.accept(initial);
            check(projections===2&&last.tiles[0]==="battery"&&store.revision===1,"late old reply cannot revert committed projection");
        """,
            publisher=True,
        )

    def test_publisher_shared_subscription_avoids_get_and_recovers_late_reconnect_new_boot(
        self,
    ):
        self.qml(
            r"""
            const topic=Qt.createQmlObject('import QtQuick; QtObject {property var data:null;property bool fresh:false}',root);
            const client=Qt.createQmlObject('import QtQuick; QtObject {property bool enabled:true;property bool connected:true;property string boot:"b1";property var topicObject:null;property int calls:0;signal newBoot(string id);function topic(name){return topicObject;}function subscribe(name){}function unsubscribe(name){}function call(method,params,callback){calls++;return calls;}}',root);
            client.topicObject=topic;
            const component=Qt.createComponent("utils/NacreStateFile.qml");
            check(component.status===Component.Ready,component.errorString());
            const store=component.createObject(root,{daemonClient:client,storeId:"control-center"});
            let projections=0,last=null;store.stateReceived.connect(value=>{projections++;last=JSON.parse(value)});
            check(client.calls===0,"subscription owns initial read");
            topic.data={store:"control-center",rev:4,exists:true,dirty:false,data:{tiles:["vpn"]}};topic.fresh=true;
            store.refresh();check(projections===1&&client.calls===0,"late snapshot projects once");
            topic.fresh=false;client.connected=false;client.connected=true;
            topic.data={store:"control-center",rev:5,exists:true,dirty:false,data:{tiles:["battery"]}};topic.fresh=true;
            check(projections===2&&last.tiles[0]==="battery"&&client.calls===0,"reconnect waits for committed snapshot");
            topic.fresh=false;client.boot="b2";client.newBoot("b2");
            topic.data={store:"control-center",rev:0,exists:true,dirty:false,data:{tiles:["volume"]}};topic.fresh=true;
            check(projections===3&&last.tiles[0]==="volume"&&store.revision===0&&client.calls===0,"new boot replaces old revision namespace");
        """,
            publisher=True,
        )

    def test_publisher_rejects_same_field_superseded_after_ack(self):
        result = self.qml(
            r"""
            const topic=Qt.createQmlObject('import QtQuick; QtObject {property var data:null; }',root);
            const client=Qt.createQmlObject('import QtQuick; QtObject {property bool enabled:true;property bool connected:true;property string boot:"b1";property var topicObject:null;property var calls:[];signal newBoot(string id);function topic(name){return topicObject;}function subscribe(name){}function unsubscribe(name){}function call(method,params,callback){calls.push({method:method,params:params,callback:callback});return calls.length;}}',root);
            client.topicObject=topic;
            const stateComponent=Qt.createComponent("utils/NacreStateFile.qml");
            check(stateComponent.status===Component.Ready,stateComponent.errorString());
            const store=stateComponent.createObject(root,{daemonClient:client,storeId:"control-center"});
            let savedCount=0,failedCount=0,finished=false;
            store.saved.connect(()=>savedCount++);
            store.saveFailed.connect(()=>{failedCount++;if(!finished){finished=true;Qt.callLater(()=>{topic.data={store:"control-center",rev:3,exists:true,data:{version:2,tiles:["vpn"]},dirty:false};Qt.callLater(()=>finish({saved:savedCount,failed:failedCount,publishes:client.calls.filter(c=>c.method==="settings.publish").length}));});}});
            const initial={store:"control-center",rev:0,exists:true,data:{version:2,tiles:["volume"]},dirty:false};
            check(client.calls.length===1&&client.calls[0].method==="settings.get","initial get");
            client.calls[0].callback(initial,null);
            store.saveText(JSON.stringify({version:2,tiles:["vpn"]}));
            check(client.calls.length===2&&client.calls[1].method==="settings.publish","publish dispatched once");
            client.calls[1].callback({store:"control-center",rev:1,exists:true,data:{version:2,tiles:["vpn"]},dirty:true},null);
            topic.data={store:"control-center",rev:2,exists:true,data:{version:2,tiles:["other"]},dirty:false};
            """,
            asynchronous=True,
            publisher=True,
        )
        self.assertEqual(result, {"saved": 0, "failed": 1, "publishes": 1})

    def test_publisher_accepts_unrelated_change_and_structural_object_equality(self):
        result = self.qml(
            r"""
            const topic=Qt.createQmlObject('import QtQuick; QtObject {property var data:null; }',root);
            const client=Qt.createQmlObject('import QtQuick; QtObject {property bool enabled:true;property bool connected:true;property string boot:"b1";property var topicObject:null;property var calls:[];signal newBoot(string id);function topic(name){return topicObject;}function subscribe(name){}function unsubscribe(name){}function call(method,params,callback){calls.push({method:method,params:params,callback:callback});return calls.length;}}',root);
            client.topicObject=topic;
            const stateComponent=Qt.createComponent("utils/NacreStateFile.qml");
            check(stateComponent.status===Component.Ready,stateComponent.errorString());
            const store=stateComponent.createObject(root,{daemonClient:client,storeId:"control-center"});
            let savedCount=0,failedCount=0;
            store.saved.connect(()=>finish({saved:++savedCount,failed:failedCount}));
            store.saveFailed.connect(()=>failedCount++);
            client.calls[0].callback({store:"control-center",rev:0,exists:true,data:{version:2,tiles:[]},dirty:false},null);
            store.saveText(JSON.stringify({version:2,preferences:{a:1,b:[2,3]}}));
            client.calls[1].callback({store:"control-center",rev:1,exists:true,data:{version:2,preferences:{b:[2,3],a:1}},dirty:true},null);
            topic.data={store:"control-center",rev:2,exists:true,data:{version:2,preferences:{b:[2,3],a:1},unrelated:"changed"},dirty:false};
            """,
            asynchronous=True,
            publisher=True,
        )
        self.assertEqual(result, {"saved": 1, "failed": 0})

    def test_publisher_accepts_already_clean_snapshot(self):
        result = self.qml(
            r"""
            const topic=Qt.createQmlObject('import QtQuick; QtObject {property var data:null; }',root);
            const client=Qt.createQmlObject('import QtQuick; QtObject {property bool enabled:true;property bool connected:true;property string boot:"b1";property var topicObject:null;property var calls:[];signal newBoot(string id);function topic(name){return topicObject;}function subscribe(name){}function unsubscribe(name){}function call(method,params,callback){calls.push({method:method,params:params,callback:callback});return calls.length;}}',root);
            client.topicObject=topic;
            const stateComponent=Qt.createComponent("utils/NacreStateFile.qml");
            check(stateComponent.status===Component.Ready,stateComponent.errorString());
            const store=stateComponent.createObject(root,{daemonClient:client,storeId:"control-center"});
            let savedCount=0,failedCount=0;
            store.saved.connect(()=>finish({saved:++savedCount,failed:failedCount}));
            store.saveFailed.connect(()=>failedCount++);
            client.calls[0].callback({store:"control-center",rev:0,exists:true,data:{version:2,tiles:[]},dirty:false},null);
            store.saveText(JSON.stringify({version:2,tiles:["vpn"]}));
            client.calls[1].callback({store:"control-center",rev:1,exists:true,data:{version:2,tiles:["vpn"]},dirty:false},null);
            """,
            asynchronous=True,
            publisher=True,
        )
        self.assertEqual(result, {"saved": 1, "failed": 0})

    def test_publisher_error_cannot_be_revived_by_later_clean_snapshot(self):
        result = self.qml(
            r"""
            const topic=Qt.createQmlObject('import QtQuick; QtObject {property var data:null; }',root);
            const client=Qt.createQmlObject('import QtQuick; QtObject {property bool enabled:true;property bool connected:true;property string boot:"b1";property var topicObject:null;property var calls:[];signal newBoot(string id);function topic(name){return topicObject;}function subscribe(name){}function unsubscribe(name){}function call(method,params,callback){calls.push({method:method,params:params,callback:callback});return calls.length;}}',root);
            client.topicObject=topic;
            const stateComponent=Qt.createComponent("utils/NacreStateFile.qml");
            check(stateComponent.status===Component.Ready,stateComponent.errorString());
            const store=stateComponent.createObject(root,{daemonClient:client,storeId:"control-center"});
            let savedCount=0,failedCount=0,finished=false;
            store.saved.connect(()=>savedCount++);
            store.saveFailed.connect(()=>{failedCount++;if(!finished){finished=true;Qt.callLater(()=>{topic.data={store:"control-center",rev:3,exists:true,data:{version:2,tiles:["vpn"]},dirty:false};Qt.callLater(()=>finish({saved:savedCount,failed:failedCount}));});}});
            client.calls[0].callback({store:"control-center",rev:0,exists:true,data:{version:2,tiles:[]},dirty:false},null);
            store.saveText(JSON.stringify({version:2,tiles:["vpn"]}));
            client.calls[1].callback({store:"control-center",rev:1,exists:true,data:{version:2,tiles:["vpn"]},dirty:true},null);
            topic.data={store:"control-center",rev:2,exists:true,data:{version:2,tiles:["vpn"]},dirty:false,error:{file:"control-center.json",line:0,message:"cannot save"}};
            """,
            asynchronous=True,
            publisher=True,
        )
        self.assertEqual(result, {"saved": 0, "failed": 1})

    def test_publisher_waits_for_first_save_before_dispatching_queued_value(self):
        result = self.qml(
            r"""
            const topic=Qt.createQmlObject('import QtQuick; QtObject {property var data:null; }',root);
            const client=Qt.createQmlObject('import QtQuick; QtObject {property bool enabled:true;property bool connected:true;property string boot:"b1";property var topicObject:null;property var calls:[];signal newBoot(string id);function topic(name){return topicObject;}function subscribe(name){}function unsubscribe(name){}function call(method,params,callback){calls.push({method:method,params:params,callback:callback});return calls.length;}}',root);
            client.topicObject=topic;
            const stateComponent=Qt.createComponent("utils/NacreStateFile.qml");
            check(stateComponent.status===Component.Ready,stateComponent.errorString());
            const store=stateComponent.createObject(root,{daemonClient:client,storeId:"control-center"});
            let savedCount=0,failedCount=0;
            store.saved.connect(()=>savedCount++);store.saveFailed.connect(()=>failedCount++);
            client.calls[0].callback({store:"control-center",rev:0,exists:true,data:{version:2,tiles:[]},dirty:false},null);
            store.saveText(JSON.stringify({version:2,tiles:["first"]}));
            store.saveText(JSON.stringify({version:2,tiles:["second"]}));
            check(client.calls.filter(c=>c.method==="settings.publish").length===1,"only one dispatched save");
            client.calls[1].callback({store:"control-center",rev:1,exists:true,data:{version:2,tiles:["first"]},dirty:true},null);
            check(savedCount===0,"accepted dirty snapshot is not saved");
            topic.data={store:"control-center",rev:1,exists:true,data:{version:2,tiles:["first"]},dirty:false};
            Qt.callLater(()=>{
                const publishes=client.calls.filter(c=>c.method==="settings.publish");
                check(publishes.length===2&&publishes[1].params.rev===1,"queued write uses fresh durable revision");
                publishes[1].callback({store:"control-center",rev:2,exists:true,data:{version:2,tiles:["second"]},dirty:false},null);
                finish({saved:savedCount,failed:failedCount,publishes:publishes.length});
            });
            """,
            asynchronous=True,
            publisher=True,
        )
        self.assertEqual(result, {"saved": 2, "failed": 0, "publishes": 2})

    def test_publisher_disconnect_ignores_late_callback_without_replay(self):
        result = self.qml(
            r"""
            const topic=Qt.createQmlObject('import QtQuick; QtObject {property var data:null; }',root);
            const client=Qt.createQmlObject('import QtQuick; QtObject {property bool enabled:true;property bool connected:true;property string boot:"b1";property var topicObject:null;property var calls:[];signal newBoot(string id);function topic(name){return topicObject;}function subscribe(name){}function unsubscribe(name){}function call(method,params,callback){calls.push({method:method,params:params,callback:callback});return calls.length;}}',root);
            client.topicObject=topic;
            const stateComponent=Qt.createComponent("utils/NacreStateFile.qml");
            check(stateComponent.status===Component.Ready,stateComponent.errorString());
            const store=stateComponent.createObject(root,{daemonClient:client,storeId:"control-center"});
            let savedCount=0,failedCount=0;
            store.saved.connect(()=>savedCount++);store.saveFailed.connect(()=>{failedCount++;Qt.callLater(()=>finish({saved:savedCount,failed:failedCount,publishes:client.calls.filter(c=>c.method==="settings.publish").length}));});
            client.calls[0].callback({store:"control-center",rev:0,exists:true,data:{version:2,tiles:[]},dirty:false},null);
            store.saveText(JSON.stringify({version:2,tiles:["vpn"]}));
            const old=client.calls[1].callback;
            client.connected=false;
            old({store:"control-center",rev:1,exists:true,data:{version:2,tiles:["vpn"]},dirty:false},null);
            """,
            asynchronous=True,
            publisher=True,
        )
        self.assertEqual(result, {"saved": 0, "failed": 1, "publishes": 1})

    def test_publisher_new_boot_terminates_ack_and_ignores_old_reply(self):
        result = self.qml(
            r"""
            const topic=Qt.createQmlObject('import QtQuick; QtObject {property var data:null; }',root);
            const client=Qt.createQmlObject('import QtQuick; QtObject {property bool enabled:true;property bool connected:true;property string boot:"b1";property var topicObject:null;property var calls:[];signal newBoot(string id);function topic(name){return topicObject;}function subscribe(name){}function unsubscribe(name){}function call(method,params,callback){calls.push({method:method,params:params,callback:callback});return calls.length;}}',root);
            client.topicObject=topic;
            const stateComponent=Qt.createComponent("utils/NacreStateFile.qml");
            check(stateComponent.status===Component.Ready,stateComponent.errorString());
            const store=stateComponent.createObject(root,{daemonClient:client,storeId:"control-center"});
            let savedCount=0,failedCount=0;
            store.saved.connect(()=>savedCount++);store.saveFailed.connect(()=>{failedCount++;Qt.callLater(()=>finish({saved:savedCount,failed:failedCount,publishes:client.calls.filter(c=>c.method==="settings.publish").length}));});
            client.calls[0].callback({store:"control-center",rev:0,exists:true,data:{version:2,tiles:[]},dirty:false},null);
            store.saveText(JSON.stringify({version:2,tiles:["vpn"]}));
            const old=client.calls[1].callback;
            client.boot="b2";client.newBoot("b2");
            old({store:"control-center",rev:1,exists:true,data:{version:2,tiles:["vpn"]},dirty:false},null);
            """,
            asynchronous=True,
            publisher=True,
        )
        self.assertEqual(result, {"saved": 0, "failed": 1, "publishes": 1})
