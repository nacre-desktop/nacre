import QtQuick
import QtTest
import qs.modules.settings.prototype
import "fixtures"
import "production/modules/settings/registry/registry.js" as Engine

TestCase {
    id: test
    name: "SettingsPrototype"
    width: 1140
    height: 900
    visible: true
    when: windowShown
    property var writes: []
    property var resets: []
    property int closes: 0
    SettingsRegistry {
        id: testRegistry
        fragmentNames: []
        documents: RegistryData.documents
        messages: RegistryData.messages
    }
    SettingsStore {
        id: testStore
        schema: testRegistry.schema
        capabilities: ({
                battery: true,
                ai: true,
                brain: true,
                bluetooth: true
            })
        availableStores: ["desktop.json", "wallpaper-picker.json", "state:scheme.json", "dashboard.json", "control-center.json", "launcher.json"]
        files: ({})
        onWriteRequested: (setting, value) => {
            test.writes = [...test.writes,
                {
                    id: setting.id,
                    value: value
                }
            ];

            testStore.accept(setting.store.file, Engine.writePath(testStore.files[setting.store.file], setting.store.key, value));
        }
        onResetRequested: settings => test.resets = settings.map(item => item.id)
    }
    Component {
        id: viewComponent
        SettingsView {
            width: 1064
            height: 764
            registry: testRegistry
            store: testStore
            capabilities: testStore.capabilities
            onCloseRequested: test.closes++
        }
    }
    function init() {
        testStore.files = {
            "desktop.json": {
                clickEdgeMenus: false,
                detachablePanels: true,
                dragPanels: true
            },
            "launcher.json": {
                recent: ["demo"],
                usage: {
                    demo: 2
                }
            }
        };
        testStore.profileValues = {};
        writes = [];
        resets = [];
        closes = 0;
        SettingsPreviewContext.store = testStore;
    }
    function test_user_only_switches_segments_and_profile_dependency() {
        const view = createTemporaryObject(viewComponent, test);
        wait(30);
        compare(writes.length, 0);
        verify(view.open("panels#clickEdgeMenus"));
        wait(30);
        const click = findChild(view, "option-panels.lips.clickEdgeMenus-true");
        verify(click);
        mouseClick(click, click.width / 2, click.height / 2);
        compare(writes.length, 1);
        compare(writes[0].value, true);
        testStore.accept("desktop.json", {
            clickEdgeMenus: false,
            detachablePanels: true,
            dragPanels: false
        });
        compare(writes.length, 1);
        verify(view.open("panels#detachablePanels"));
        wait(30);
        const toggle = findChild(view, "control-panels.lips.detachablePanels");
        verify(toggle.enabled);
        mouseClick(toggle, toggle.width / 2, toggle.height / 2);
        compare(writes.length, 2);
        compare(writes[1].value, false);
        verify(!findChild(view, "control-panels.lips.dragPanels").enabled);
        testStore.profileValues = {
            "panels.lips.detachablePanels": true
        };
        wait(10);
        verify(!toggle.enabled);
        verify(findChild(view, "panels.lips.detachablePanels").profile);
        compare(writes.length, 2);
    }
    function test_search_keyboard_highlight_escape_and_narrow() {
        const view = createTemporaryObject(viewComponent, test, {
            width: 620
        });
        verify(view.narrow);
        view.forceActiveFocus();
        keyClick(Qt.Key_P);
        compare(view.query, "p");
        view.query = "pearls";
        wait(30);
        verify(view.results.length > 0);
        const target = view.results[0];
        view.focusSearch();
        keyClick(Qt.Key_Return);
        wait(30);
        compare(view.page, target.page);
        compare(view.highlight, target.id);
        verify(findChild(view, target.id).highlighted);
        view.query = "hover";
        view.forceActiveFocus();
        keyClick(Qt.Key_Escape);
        compare(view.query, "");
        compare(closes, 0);
        keyClick(Qt.Key_Escape);
        compare(closes, 1);
        verify(findChild(view, "registrySearch").width <= view.width);
    }
    function test_routes_gating_reset_and_invalid_writes() {
        const view = createTemporaryObject(viewComponent, test);
        verify(!view.open("panels#does-not-exist"));
        verify(view.open("maintenance"));
        compare(view.page, "system");
        verify(view.open("panels"));
        testStore.profileValues = {
            "panels.lips.detachablePanels": true
        };
        testStore.reset("panels");
        verify(resets.includes("panels.lips.clickEdgeMenus"));
        verify(!resets.includes("panels.lips.detachablePanels"));
        verify(!resets.includes("panels.lips.hoverDelay"));
        const planned = testRegistry.schema.settings.find(item => item.id === "power.saver.enabled");
        verify(!testStore.write(planned, true));
        const click = testRegistry.schema.settings.find(item => item.id === "panels.lips.clickEdgeMenus");
        verify(!testStore.write(click, "true"));
        testStore.capabilities = {};
        verify(!view.open("power"));
        verify(!view.open("panels/ai-sidebar"));
        verify(view.open("ai"));
        compare(view.page, "addons");
        testStore.capabilities = {
            battery: true,
            ai: true,
            brain: true,
            bluetooth: true
        };
    }
    function test_history_clear_requires_confirmation() {
        const view = createTemporaryObject(viewComponent, test);
        view.open("launcher");
        wait(30);
        const action = testRegistry.schema.actions.find(item => item.id === "launcher.history");
        view.action(action);
        verify(!!view.pendingAction);
        compare(testStore.files["launcher.json"].recent.length, 1);
        view.forceActiveFocus();
        keyClick(Qt.Key_Escape);
        verify(!view.pendingAction);
        compare(testStore.files["launcher.json"].recent.length, 1);
    }
    SignalSpy {
        id: confirmationSpy
        signalName: "confirmationRequested"
    }
    SignalSpy {
        id: writeSpy
        signalName: "writeRequested"
    }
    Component {
        id: independentStore
        SettingsStore {}
    }
    function test_confirmation_uses_owned_path_and_waits_for_provider_ack() {
        const source = testRegistry.schema.settings.find(item => item.id === "panels.lips.clickEdgeMenus");
        const confirmed = Object.assign({}, source, {
            id: "panels.lips.confirmed",
            key: "confirmed",
            type: "boolean",
            default: false,
            apply: "confirm",
            store: {
                file: "desktop.json",
                key: ["confirmed"]
            }
        });
        const schema = Object.assign({}, testRegistry.schema, {
            settings: [...testRegistry.schema.settings, confirmed]
        });
        const owner = createTemporaryObject(independentStore, test, {
            schema: schema,
            availableStores: ["desktop.json"],
            capabilities: testStore.capabilities
        });
        confirmationSpy.target = owner;
        writeSpy.target = owner;
        confirmationSpy.clear();
        writeSpy.clear();
        verify(!owner.write(confirmed, true));
        compare(confirmationSpy.count, 1);
        compare(writeSpy.count, 0);
        compare(owner.value(confirmed), false);
        const forged = Object.assign({}, confirmed, {
            store: {
                file: "unowned.json",
                key: ["bad"]
            }
        });
        verify(owner.write(forged, true, true));
        compare(writeSpy.count, 1);
        compare(writeSpy.signalArguments[0][0].store.file, "desktop.json");
        compare(owner.value(confirmed), false);
        owner.accept("desktop.json", {
            confirmed: true
        });
        compare(owner.value(confirmed), true);
        confirmationSpy.target = null;
        writeSpy.target = null;
    }
    function test_rail_arrows_and_enter_open_page() {
        const view = createTemporaryObject(viewComponent, test);
        wait(30);
        const nav = findChild(view, "registryNavigation");
        const row = nav.itemAtIndex(nav.currentIndex);
        const button = row.children.find(item => typeof item.activate === "function");
        button.forceActiveFocus();
        keyClick(Qt.Key_Down);
        wait(20);
        compare(nav.currentIndex, 4);
        keyClick(Qt.Key_Return);
        wait(20);
        compare(view.page, "launcher");
    }
    function test_preview_persistence_newer_version_and_failure() {
        const backend = createTemporaryObject(backendComponent, test);
        const file = findChild(backend, "settingsPreviewFile");
        file.textValue = JSON.stringify({
            version: 1,
            files: {
                "desktop.json": {
                    clickEdgeMenus: false,
                    unknown: "kept"
                }
            }
        });
        file.loaded();
        verify(backend.ready && backend.writable);
        verify(backend.change("desktop.json", ["clickEdgeMenus"], true));
        compare(testStore.files["desktop.json"].clickEdgeMenus, true);
        compare(JSON.parse(file.textValue).files["desktop.json"].unknown, "kept");
        const before = file.writes;
        file.textValue = JSON.stringify({
            version: 2,
            files: {}
        });
        file.loaded();
        verify(!backend.writable);
        verify(!backend.change("desktop.json", ["clickEdgeMenus"], false));
        compare(file.writes, before);
        compare(JSON.parse(file.textValue).version, 2);
        verify(testStore.error.length > 0);
        testStore.error = "";
    }
    Component {
        id: backendComponent
        PreviewBackend {
            store: testStore
            path: "preview-fixture.json"
        }
    }
    function test_reserved_packages_and_shared_editor_routes() {
        const view = createTemporaryObject(viewComponent, test);
        verify(!view.sections.some(item => item.id === "launcher.packages"));
        view.open("panels/dashboard#pages");
        wait(50);
        compare(view.highlight, "panels/dashboard.editor.pages");
        verify(findChild(view, "panels/dashboard.editor.pages").item);
        view.open("panels/control-center#tiles");
        wait(50);
        verify(findChild(view, "panels/control-center.editor.tiles").item);
        view.open("launcher");
        verify(!view.sections.some(item => item.id === "launcher.packages"));
    }
}
