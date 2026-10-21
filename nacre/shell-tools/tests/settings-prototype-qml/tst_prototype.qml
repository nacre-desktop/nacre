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
                panelDragging: true,
                ai: true,
                brain: true,
                bluetooth: true
            })
        availableStores: ["desktop", "wallpaper-picker", "scheme", "dashboard", "dashboard-extras", "control-center", "launcher"]
        files: ({})
        onWriteRequested: (setting, value) => {
            test.writes = [...test.writes,
                {
                    id: setting.id,
                    value: value
                }
            ];

            testStore.accept(setting.store.id, Engine.writePath(testStore.files[setting.store.id], setting.store.key, value));
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
        testStore.capabilities = {
            battery: true,
            ai: true,
            brain: true,
            bluetooth: true,
            panelDragging: true
        };
        testStore.files = {
            "desktop": {
                clickEdgeMenus: false,
                detachablePanels: true,
                dragPanels: true
            },
            "launcher": {
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
    Component {
        id: choiceComponent
        NacreSelect {
            width: 280
            x: 30
            y: 30
            searchable: true
            model: Array(60).fill(0).map((_, index) => "Choice " + index)
            currentIndex: 30
        }
    }
    function test_dropdown_search_selection_and_wheel_speed() {
        const control = createTemporaryObject(choiceComponent, test);
        mouseClick(control, control.width / 2, control.height / 2);
        tryCompare(control.popup, "visible", true);
        const list = findChild(control.popup.contentItem, "choiceList");
        list.contentY = 0;
        mouseWheel(list, list.width / 2, list.height / 2, 0, -120);
        tryVerify(() => list.contentY > 80);
        const filter = findChild(control.popup.contentItem, "choiceFilter");
        filter.text = "Choice 3";
        filter.forceActiveFocus();
        keyClick(Qt.Key_Return);
        compare(control.currentText, "Choice 3");
        tryCompare(control.popup, "visible", false);
    }
    function test_default_application_picker_writes_installed_argv() {
        testStore.availableStores = [...testStore.availableStores, "apps"];
        testStore.accept("apps", {
            browser: ["custom-route"]
        });
        const view = createTemporaryObject(viewComponent, test);
        view.open("apps");
        wait(30);
        const button = findChild(view, "control-apps.defaults.browser");
        verify(button);
        mouseClick(button, button.width / 2, button.height / 2);
        tryVerify(() => view.entitySetting?.id === "apps.defaults.browser");
        const picker = findChild(view, "defaultApplicationPicker");
        verify(picker);
        picker.activated(1);
        compare(writes[writes.length - 1].value.join("|"), "demo-editor");
    }
    function test_gradient_palette_preference_and_text_scale() {
        const old = testStore.files.desktop;
        testStore.accept("desktop", Object.assign({}, old, {
            interfaceGradients: false,
            textSize: 14
        }));
        compare(NacreTokens.gradientsEnabled, false);
        compare(NacreTokens.gradientColour("#123456", "#abcdef"), Qt.color("#123456"));
        compare(NacreTokens.textSize.body, 14);
        testStore.accept("desktop", Object.assign({}, old, {
            interfaceGradients: true
        }));
        compare(NacreTokens.gradientColour("#123456", "#abcdef"), Qt.color("#abcdef"));
        testStore.accept("desktop", old);
    }
    function test_back_returns_to_origin_of_privacy_links() {
        const view = createTemporaryObject(viewComponent, test);
        view.open("privacy");
        view.open("panels/dashboard#weatherPhotos");
        compare(view.page, "panels/dashboard/weather");
        view.back();
        compare(view.page, "privacy");
        view.open("panels/dashboard#lyrics");
        view.back();
        compare(view.page, "privacy");
    }
    function test_publisher_manifest_switches_share_real_output_preferences() {
        testStore.availableStores = [...testStore.availableStores, "colors"];
        NacreSettingsStorage.documents = {
            "theme-targets": {
                targets: [
                    {
                        id: "gtk3",
                        labelId: "settings.publisher.gtk3",
                        status: "current"
                    },
                    {
                        id: "thunar",
                        labelId: "settings.publisher.thunar",
                        status: "current"
                    },
                    {
                        id: "kitty",
                        labelId: "settings.publisher.kitty",
                        status: "current"
                    },
                    {
                        id: "chromium",
                        labelId: "settings.publisher.chromium",
                        status: "current"
                    },
                    {
                        id: "discord",
                        labelId: "settings.publisher.discord",
                        status: "current"
                    },
                    {
                        id: "spotify",
                        labelId: "settings.publisher.spotify",
                        status: "current"
                    }
                ]
            }
        };
        const view = createTemporaryObject(viewComponent, test);
        verify(view.open("colors/app-theming"));
        wait(30);
        const gtk = findChild(view, "control-colors/app-theming.targets.gtk3");
        const thunar = findChild(view, "control-colors/app-theming.targets.thunar");
        const chromium = findChild(view, "control-colors/app-theming.targets.chromium");
        const discord = findChild(view, "control-colors/app-theming.targets.discord");
        const spotify = findChild(view, "control-colors/app-theming.targets.spotify");
        const chromiumRow = findChild(view, "colors/app-theming.targets.chromium");
        const discordRow = findChild(view, "colors/app-theming.targets.discord");
        const spotifyRow = findChild(view, "colors/app-theming.targets.spotify");
        verify(gtk && thunar && chromium && discord && spotify);
        verify(chromiumRow && discordRow && spotifyRow);
        compare(chromiumRow.description, RegistryData.messages["settings.registry.colors.target.chromium.description"]);
        compare(discordRow.description, RegistryData.messages["settings.registry.colors.target.discord.description"]);
        compare(spotifyRow.description, RegistryData.messages["settings.registry.colors.target.spotify.description"]);
        verify(gtk.checked && thunar.checked && chromium.checked);
        verify(discord.checked && spotify.checked);
        mouseClick(gtk, gtk.width / 2, gtk.height / 2);
        compare(testStore.files.colors.appThemes.gtk3, false);
        verify(!thunar.checked);
        mouseClick(chromium, chromium.width / 2, chromium.height / 2);
        compare(testStore.files.colors.appThemes.chromium, false);
        mouseClick(discord, discord.width / 2, discord.height / 2);
        compare(testStore.files.colors.appThemes.discord, false);
        mouseClick(spotify, spotify.width / 2, spotify.height / 2);
        compare(testStore.files.colors.appThemes.spotify, false);
        mouseClick(thunar, thunar.width / 2, thunar.height / 2);
        verify(gtk.checked && thunar.checked);
        verify(view.results.length === 0);
        view.query = "Thunar";
        verify(view.results.some(item => item.id === "colors/app-theming.targets.thunar"));
        view.query = "Chromium";
        verify(view.results.some(item => item.id === "colors/app-theming.targets.chromium"));
        NacreSettingsStorage.documents = {};
    }
    function test_power_master_switch_unlocks_real_profile_choices() {
        testStore.availableStores = [...testStore.availableStores, "power"];
        testStore.capabilities = Object.assign({}, testStore.capabilities, {
            powerProfiles: true,
            performanceProfile: true
        });
        testStore.accept("power", {
            enabled: false,
            ac: "balanced",
            battery: "power-saver"
        });
        const view = createTemporaryObject(viewComponent, test);
        view.open("power");
        wait(30);
        const toggle = findChild(view, "control-power.profiles.enabled");
        const ac = findChild(view, "option-power.profiles.ac-performance");
        verify(toggle.enabled);
        verify(!ac.enabled);
        mouseClick(toggle, toggle.width / 2, toggle.height / 2);
        tryVerify(() => testStore.files.power.enabled === true);
        verify(ac.enabled);
        mouseClick(ac, ac.width / 2, ac.height / 2);
        tryCompare(testStore.files.power, "ac", "performance");
        const battery = findChild(view, "option-power.profiles.battery-balanced");
        mouseClick(battery, battery.width / 2, battery.height / 2);
        tryCompare(testStore.files.power, "battery", "balanced");
    }
    function test_root_navigation_has_no_history_and_entity_back_collapses_loader() {
        const view = createTemporaryObject(viewComponent, test);
        view.open("panels");
        view.open("launcher");
        compare(view.backStack.length, 0);
        verify(!findChild(view, "registryBack").visible);
        const setting = testRegistry.schema.settings.find(row => row.id === "launcher/favorites.list.favorites");
        verify(setting);
        view.entitySetting = setting;
        wait(30);
        const loader = findChild(view, "registryEntity");
        verify(loader.height > 0);
        view.entitySetting = null;
        wait(30);
        verify(!loader.visible);
        compare(loader.height, 0);
        view.open("colors");
        const scroll = findChild(view, "registryScroll");
        wait(30);
        compare(scroll.contentY, 0);
        verify(scroll.contentHeight < 2500);
    }
    function test_user_only_switches_segments_and_profile_dependency() {
        const view = createTemporaryObject(viewComponent, test);
        wait(30);
        compare(writes.length, 0);
        verify(view.open("panels#clickEdgeMenus"));
        wait(30);
        const click = findChild(view, "option-panels.lips.clickEdgeMenus-true");
        verify(click);
        const hover = findChild(view, "option-panels.lips.clickEdgeMenus-false");
        verify(click.y === hover.y, "group=" + click.parent.width + " hover=" + hover.width + " click=" + click.width);
        mouseClick(click, click.width / 2, click.height / 2);
        compare(writes.length, 1);
        compare(writes[0].value, true);
        testStore.accept("desktop", {
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
    function test_disabled_controls_and_right_alignment_at_both_widths() {
        testStore.profileValues = {
            "panels.lips.detachablePanels": true,
            "panels.lips.hoverDelay": 350,
            "colors.palette.mode": "dark",
            "colors.palette.style": "natural"
        };
        for (const width of [1064, 620]) {
            const view = createTemporaryObject(viewComponent, test, {
                width: width
            });
            for (const route of ["panels", "colors"]) {
                verify(view.open(route));
                wait(30);
                const ids = route === "panels" ? ["panels.lips.detachablePanels", "panels.lips.hoverDelay"] : ["colors.palette.mode", "colors.palette.style"];
                for (const id of ids) {
                    const row = findChild(view, id);
                    const control = findChild(view, (id === "colors.palette.mode" ? "option-" + id + "-light" : "control-" + id));
                    verify(control);
                    verify(!control.enabled, id);
                    const editor = row.children.find(child => child.toString().startsWith("QQuickLoader"));
                    verify(editor);
                    compare(editor.opacity, .45);
                    fuzzyCompare(editor.x + editor.width, row.width - 8, .01);
                    const before = writes.length;
                    mouseClick(control, control.width / 2, control.height / 2);
                    compare(writes.length, before, id);
                }
            }
            view.destroy();
        }
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
        verify(resets.includes("panels.lips.hoverDelay"));
        const planned = testRegistry.schema.settings.find(item => item.id === "power.saver.batterySaver");
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
        compare(testStore.files["launcher"].recent.length, 1);
        view.forceActiveFocus();
        keyClick(Qt.Key_Escape);
        verify(!view.pendingAction);
        compare(testStore.files["launcher"].recent.length, 1);
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
                id: "desktop",
                key: ["confirmed"]
            }
        });
        const schema = Object.assign({}, testRegistry.schema, {
            settings: [...testRegistry.schema.settings, confirmed]
        });
        const owner = createTemporaryObject(independentStore, test, {
            schema: schema,
            availableStores: ["desktop"],
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
                id: "unowned",
                key: ["bad"]
            }
        });
        verify(owner.write(forged, true, true));
        compare(writeSpy.count, 1);
        compare(writeSpy.signalArguments[0][0].store.id, "desktop");
        compare(owner.value(confirmed), false);
        owner.accept("desktop", {
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
                "desktop": {
                    clickEdgeMenus: false,
                    unknown: "kept"
                }
            }
        });
        file.loaded();
        verify(backend.ready && backend.writable);
        verify(backend.change("desktop", ["clickEdgeMenus"], true));
        compare(testStore.files["desktop"].clickEdgeMenus, true);
        compare(JSON.parse(file.textValue).files["desktop"].unknown, "kept");
        const before = file.writes;
        file.textValue = JSON.stringify({
            version: 2,
            files: {}
        });
        file.loaded();
        verify(!backend.writable);
        verify(!backend.change("desktop", ["clickEdgeMenus"], false));
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
        compare(view.highlight, "panels/dashboard/pages.editor.pages");
        verify(findChild(view, "panels/dashboard/pages.editor.pages").item);
        view.open("panels/control-center#tiles");
        wait(50);
        verify(findChild(view, "panels/control-center.editor.tiles").item);
        view.open("launcher");
        verify(!view.sections.some(item => item.id === "launcher.packages"));
    }
}
