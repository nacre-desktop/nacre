import QtQuick
import QtTest
import "calculator.js" as Arithmetic
import "fixtures"

TestCase {
    id: test

    function init() {
        mouseMove(test, width - 2, height - 2);
        LauncherPreferences.favorites = [];
        LauncherPreferences.hidden = [];
        LauncherPreferences.recent = [];
        NacreApps.launched = "";
        AppLaunch.commands = [];
        SidebarChat.commands = [];
        SidebarChat.busy = false;
        SidebarChat.draft = "";
        NacreMenu.snapshot = {
            "version": 1,
            "capabilities": {
                "capture": true,
                "ai": true,
                "brain": true,
                "packages": true
            },
            "packages": [],
            "fonts": ["IBM Plex Sans"],
            "reminders": [],
            "webapps": []
        };
        NacreMenu.message = "";
        NacrePanelState.calls = [];
    }

    function view() {
        const value = createTemporaryObject(scene, this);
        wait(40);
        return value;
    }

    function test_shell_surface_heading_and_favorites_layout() {
        LauncherPreferences.favorites = ["editor"];
        const value = view();
        compare(value.border.width, 0);
        compare(value.color.toString(), "#00000000");
        const heart = findChild(value, "launcherFavorite_0");
        verify(!heart.visible);
        value.navigate("apps");
        const back = findChild(value, "launcherBack");
        compare(back.width, back.height);
        const heading = findChild(value, "launcherHeading");
        compare(Math.round(back.y + back.height / 2), Math.round(heading.y + heading.height / 2));
    }

    function test_files_reject_stale_results_and_conversion_uses_owner() {
        const value = view(), field = findChild(value, "launcherSearch");
        NacreMenu.fileRows = [
            {
                "id": "one",
                "name": "Report.pdf",
                "path": "/fixture/Report.pdf"
            }
        ];
        NacreMenu.fileQuery = "report";
        field.text = ": report";
        wait(20);
        compare(value.entries[0].name, "Report.pdf");
        field.text = ": other";
        wait(20);
        compare(value.entries.length, 0);
        NacreMenu.conversionQuery = "10 km to miles";
        NacreMenu.conversion = {
            "ok": true,
            "value": "6.2137 mi"
        };
        field.text = "= 10 km to miles";
        wait(20);
        compare(value.entries[0].value, "6.2137 mi");
    }

    function test_hover_heart_and_action_anchor_stay_inside_drawer() {
        LauncherPreferences.favorites = ["editor"];
        const value = view();
        const tile = findChild(value, "launcherTile_0"), heart = findChild(value, "launcherFavorite_0");
        mouseMove(value, 5, 5);
        wait(20);
        verify(!heart.visible);
        mouseMove(tile, tile.width / 2, tile.height / 2);
        tryCompare(heart, "visible", true);
        mouseClick(heart, heart.width / 2, heart.height / 2);
        tryCompare(value, "entries", []);
        value.navigate("apps");
        value.keyboardSelection = true;
        value.selection = 0;
        wait(20);
        const trigger = findChild(value, "launcherDetailActions");
        value.actionsFor(value.selectedEntry, trigger);
        const popup = findChild(value, "launcherItemActions");
        tryCompare(popup, "opened", true);
        wait(30);
        verify(popup.height > 50, "popup height " + popup.height + " content " + popup.contentItem.implicitHeight);
        verify(popup.x + popup.width <= value.width - 8);
        verify(popup.y + popup.height <= value.height - 86);
        verify(popup.x < value.contextAnchor.x);
        compare(Math.round(popup.y), Math.round(value.contextAnchor.y));
        value.height = 340;
        value.contextEntry = Object.assign({}, value.selectedEntry, {
            reason: "Additional details ".repeat(50),
            disabled: true
        });
        wait(30);
        verify(popup.y >= 8);
        verify(popup.y + popup.height <= value.height - 86);
    }
    function test_controls_can_be_restored_and_show_authoritative_state() {
        const value = view();
        value.hoveredEntry = NacreMenu.apps[0];
        verify(!!value.previewEntry);
        value.showQuickControls();
        wait(30);
        compare(value.previewEntry, null);
        const dnd = findChild(value, "launcherToggle_dnd");
        verify(dnd.visible);
        verify(dnd.toggle);
        DesktopSettings.set("dnd", false);
        dnd.clicked();
        compare(dnd.selected, true);
        dnd.clicked();
        compare(dnd.selected, false);
        compare(value.previewEntry, null);
        const night = findChild(value, "launcherToggle_night-light");
        night.clicked();
        compare(night.selected, true);
        night.clicked();
        compare(night.selected, false);
        LauncherPreferences.recent = ["nacre-menu:actions.controls.night-light", "editor"];
        compare(NacreMenu.recent().length, 1);
        compare(NacreMenu.recent()[0].id, "editor");
    }
    function test_sections_and_five_row_apps() {
        const value = view();
        compare(NacreMenu.navigation.length, 9);
        value.navigate("apps");
        compare(value.entries.length, 4);
        const grid = findChild(value, "launcherApps");
        compare(grid.cellHeight, grid.height / 5);
        compare(value.selectedEntry, null);
        value.navigate("actions");
        verify(value.entries.some(row => {
            return row.id === "actions.capture";
        }));
        verify(!NacreMenu.navigation.some(row => {
            return ["games", "office", "development"].includes(row.id);
        }));
    }

    function test_query_selects_and_launches_real_app_owner() {
        const value = view(), field = findChild(value, "launcherSearch");
        field.text = "google";
        wait(20);
        compare(value.selectedEntry.name, "Google Chrome");
        value.run(value.selectedEntry);
        compare(NacreApps.launched, "google-chrome");
        compare(value.visibilities.launcher, false);
    }

    function test_no_idle_selection_and_alphabetical_mixed_favorites() {
        LauncherPreferences.favorites = ["music", "editor", "nacre-menu:actions.terminal"];
        const value = view();
        compare(value.entries.map(row => {
            return row.name;
        }).join(","), "Editor,Music,Terminal");
        compare(value.selection, -1);
        value.activate(0);
        compare(NacreApps.launched, "");
        value.focusResults();
        compare(value.selection, 0);
        value.favorite(value.entries[0]);
        verify(!LauncherPreferences.favorites.includes("editor"));
    }

    function test_deep_navigation_search_scope_and_back() {
        const value = view(), field = findChild(value, "launcherSearch");
        value.navigate("style.personality");
        verify(value.entries.some(row => {
            return row.value === "pearl";
        }));
        value.run(value.entries.find(row => {
            return row.value === "pearl";
        }));
        compare(NacreWallpapers.appearancePreferences.palettePersonality, "pearl");
        value.back();
        compare(value.route, "home");
        field.text = "> wifi";
        wait(20);
        compare(value.selectedEntry.id, "setup.network");
        value.run(value.selectedEntry);
        compare(NacrePanelState.calls[0][1], "network");
    }

    function test_calculator_and_explicit_ai_guard() {
        const value = view(), field = findChild(value, "launcherSearch");
        field.text = "= (12+8)/5";
        wait(20);
        compare(value.selectedEntry.value, "4");
        value.run(value.selectedEntry);
        compare(AppLaunch.commands[0][0], "wl-copy");
        value.visibilities.launcher = true;
        field.text = "? Explain shells";
        wait(20);
        verify(!value.selectedEntry.disabled);
        SidebarChat.busy = true;
        wait(20);
        verify(value.entries[0].disabled);
        value.run(value.entries[0]);
        compare(SidebarChat.commands.length, 0);
    }

    function test_power_requires_confirmation_and_route_never_executes() {
        const value = view();
        value.navigate("power.poweroff");
        compare(AppLaunch.commands.length, 0);
        compare(value.route, "power");
        const shutdown = value.entries.find(row => {
            return row.value === "poweroff";
        });
        value.run(shutdown);
        verify(!!value.confirmation);
        compare(AppLaunch.commands.length, 0);
        value.dismiss();
        compare(value.confirmation, null);
        compare(value.visibilities.launcher, true);
    }

    function test_hidden_restore_and_live_toggle() {
        const value = view();
        LauncherPreferences.hidden = ["music"];
        value.navigate("hidden");
        compare(value.entries.length, 1);
        value.run(value.entries[0]);
        compare(LauncherPreferences.hidden.length, 0);
        value.navigate("actions.controls");
        const dnd = value.entries.find(row => {
            return row.value === "dnd";
        });
        value.run(dnd);
        verify(DesktopSettings.data.dnd);
        compare(value.visibilities.launcher, true);
    }

    function test_font_and_wallpaper_scoped_search() {
        const value = view(), field = findChild(value, "launcherSearch");
        value.navigate("style.fonts");
        field.text = "Plex";
        wait(20);
        verify(value.entries.some(row => {
            return row.value === "IBM Plex Sans";
        }));
        value.navigate("style.wallpapers");
        field.text = "garden";
        wait(20);
        compare(value.entries[0].name, "Blue garden");
    }

    function test_escape_without_mouse_and_small_layout() {
        const value = view();
        const field = findChild(value, "launcherSearch");
        field.forceActiveFocus();
        keyClick(Qt.Key_Escape);
        compare(value.visibilities.launcher, false);
        value.visibilities.launcher = true;
        value.width = 760;
        value.height = 480;
        value.navigate("apps");
        verify(!value.showDetails);
        const grid = findChild(value, "launcherApps");
        compare(grid.cellHeight, grid.height / 5);
        verify(grid.x + grid.width <= value.width);
    }

    function test_legacy_favorite_and_branded_icon_contract() {
        LauncherPreferences.favorites = ["nacre-action:terminal:"];
        const value = view();
        compare(value.entries[0].name, "Open terminal");
        value.navigate("apps");
        wait(30);
        const grid = findChild(value, "launcherApps");
        const aiIndex = value.entries.findIndex(entry => {
            return entry.id === "nacre-ai";
        });
        const editorIndex = value.entries.findIndex(entry => {
            return entry.id === "editor";
        });
        const ai = findChild(grid, "launcherTile_" + aiIndex), editor = findChild(grid, "launcherTile_" + editorIndex);
        verify(findChild(ai, "nacreAiAppLogo").visible);
        verify(findChild(ai, "nacreAiAppLogo").ai);
        verify(!findChild(editor, "nacreAiAppLogo").visible);
    }

    function test_ctrl_d_from_search_and_favorite_keeps_apps_selection() {
        const value = view(), field = findChild(value, "launcherSearch");
        field.text = "editor";
        wait(20);
        field.forceActiveFocus();
        keyClick(Qt.Key_D, Qt.ControlModifier);
        verify(LauncherPreferences.favorites.includes("editor"));
        value.navigate("apps");
        value.focusResults();
        const before = value.selectedEntry.id;
        value.favorite(value.selectedEntry);
        compare(value.selectedEntry.id, before);
        compare(value.route, "apps");
    }

    function test_blank_actions_and_unmatched_text_never_run_or_send() {
        const value = view(), field = findChild(value, "launcherSearch");
        field.text = ">";
        wait(20);
        value.activate(0);
        compare(AppLaunch.commands.length, 0);
        compare(SidebarChat.commands.length, 0);
        field.text = "nothing matches this query";
        wait(20);
        compare(value.entries.length, 1);
        compare(value.entries[0].operation, "search-web");
        compare(AppLaunch.commands.length, 0);
        value.activate(0);
        compare(SidebarChat.commands.length, 0);
        compare(AppLaunch.commands[0][0], "nacre-app");
        verify(AppLaunch.commands[0][2].endsWith("nothing%20matches%20this%20query"));
    }

    function test_arithmetic_grammar_stays_bounded() {
        const valid = [["2+3*4", "14"], ["(2+3)*4", "20"], ["2^3^2", "512"], ["sqrt(81)", "9"], ["-2^2", "-4"], ["abs(-3)", "3"], ["round(1.6)", "2"]];
        for (const pair of valid) {
            const result = Arithmetic.evaluate(pair[0]);
            verify(result.ok);
            compare(result.value, pair[1]);
        }
        for (const expression of ["1/0", "process.exit()", "Math.random()", "1+", "(".repeat(40) + "1" + ")".repeat(40)])
            verify(!Arithmetic.evaluate(expression).ok);
    }

    function test_notes_never_show_results_from_a_previous_query() {
        const value = view(), field = findChild(value, "launcherSearch");
        DesktopExtras.notes = [
            {
                "path": "old.md",
                "title": "Old result",
                "preview": "Old"
            }
        ];
        DesktopExtras.notesQuery = "old";
        field.text = "# current";
        wait(20);
        compare(value.entries.length, 0);
        DesktopExtras.notesQuery = "current";
        DesktopExtras.notes = [
            {
                "path": "new.md",
                "title": "Current result",
                "preview": "Current"
            }
        ];
        wait(20);
        compare(value.entries[0].name, "Current result");
    }

    function test_shared_pick_and_input_modes_return_values() {
        const value = view();
        NacreMenuPrompt.ownerView = value.visibilities;
        NacreMenuPrompt.request = {
            "id": "test",
            "kind": "pick",
            "title": "Pick one",
            "choices": [
                {
                    "label": "One",
                    "value": "one"
                },
                {
                    "label": "Two",
                    "value": "two"
                }
            ]
        };
        NacreMenuPrompt.active = true;
        value.reset();
        const field = findChild(value, "launcherSearch");
        field.text = "Two";
        wait(20);
        value.run(value.selectedEntry);
        compare(NacreMenuPrompt.result[0], "two");
        compare(NacreMenuPrompt.result[1], false);
        NacreMenuPrompt.ownerView = value.visibilities;
        NacreMenuPrompt.request = {
            "id": "test2",
            "kind": "input",
            "title": "Name",
            "initial": "Example"
        };
        NacreMenuPrompt.active = true;
        value.reset();
        wait(30);
        const input = findChild(value, "menuFormField_value");
        verify(!!input);
        compare(input.text, "Example");
        input.text = "Chosen";
        input.forceActiveFocus();
        keyClick(Qt.Key_Return);
        compare(NacreMenuPrompt.result[0], "Chosen");
        NacreMenuPrompt.ownerView = null;
        NacreMenuPrompt.active = false;
        NacreMenuPrompt.request = null;
    }

    function test_initial_cli_leaf_route_selects_parent_without_execution() {
        const value = view();
        value.visibilities.launcherRoute = "setup.displays";
        value.reset();
        compare(value.route, "setup");
        compare(value.selectedEntry.id, "setup.displays");
        compare(NacrePanelState.calls.length, 0);
        value.visibilities.launcherRoute = "power.poweroff";
        value.reset();
        compare(value.route, "power");
        compare(value.selectedEntry.id, "power.poweroff");
        compare(AppLaunch.commands.length, 0);
    }

    name: "NacreUniversalMenu"
    width: 1200
    height: 850
    visible: true
    when: windowShown

    Component {
        id: scene

        NacreAppBrowser {
            width: 1100
            height: 590

            visibilities: QtObject {
                property bool launcher: true
                property string launcherMode: "apps"
                property string launcherRoute: "home"
                property string launcherQuery: ""
                property int launcherRequest: 0
            }
        }
    }
}
