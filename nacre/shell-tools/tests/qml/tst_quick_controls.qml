import QtQuick
import QtTest
import "fixtures"
import "wifi-networks.js" as Wifi

TestCase {
    id: test

    property var activePopup: null
    property int frameEscapes: 0
    Keys.onEscapePressed: event => {
        if (activePopup) {
            activePopup.hasCurrent = false;
            frameEscapes++;
            event.accepted = true;
        }
    }
    property var groupedAPs: Wifi.group([firstAP, secondAP])

    function init() {
        NacrePanelState.screens = ({
                "test": {
                    "dashboard": false
                }
            });
        NacrePanelState.panels = ({
                "test": {
                    "popouts": {
                        "hasCurrent": true,
                        "pinned": true
                    }
                }
            });
        NacreControlActions.busy = false;
        NacreAudio.writes = 0;
        NacreAudio.available = true;
        NacreAudio.micAvailable = true;
        DesktopSettings.writes = [];
        AppLaunch.calls = [];
        DesktopActions.calls = [];
        NacreCompositor.logoutCount = 0;
        NacreNetwork.wiredConnected = false;
        NacreNetwork.wifiAvailable = true;
        NacreBrightness.device.available = true;
        NacreKeyboardLight.available = true;
        NacrePlayers.active = null;
        NacreControlPreferences.reset();
        NacreControlTools.data = ({
                screenshot: true,
                colorPicker: true,
                powerProfilesSupported: true,
                nightLightSupported: true,
                nightLightExternal: false,
                nightLightEnabled: false
            });
    }

    Component {
        id: editor
        NacreControlEdit {
            width: 560
            screen: ({
                    name: "test",
                    width: 1000,
                    height: 700
                })
        }
    }
    Component {
        id: scrollingList
        NacreQuickList {
            width: 300
            maximumHeight: 200
            Rectangle {
                width: 290
                implicitHeight: 2200
                color: "transparent"
            }
        }
    }
    Component {
        id: center
        Item {
            width: 424
            height: 860
            property alias panel: panel
            property string selectedSection: "home"
            property bool sidebarPointerOwned: false
            NacreControlCenter {
                id: panel
                width: implicitWidth
                height: implicitHeight
                screen: ({
                        name: "test"
                    })
                section: parent.selectedSection
                onSectionRequested: section => parent.selectedSection = section
                onSidebarRequested: pointerOwned => {
                    parent.sidebarPointerOwned = pointerOwned;
                    presentation = "sidebar";
                    parent.selectedSection = "home";
                }
                visibility: true
            }
        }
    }
    function test_control_center_readonly_lazy_navigation_and_small_bounds() {
        NacreAudio.writes = 0;
        const scene = createTemporaryObject(center, this), panel = scene.panel;
        wait(50);
        verify(!panel.activeFocus);
        panel.keyboardActive = true;
        tryCompare(panel, "activeFocus", true);
        compare(NacreAudio.writes, 0);
        compare(NacreControlTools.writes, 0);
        const volume = findChild(panel, "controlVolume");
        verify(!!volume && volume.width > 200);
        volume.value = .8;
        volume.moved();
        compare(NacreAudio.writes, 1);
        verify(!findChild(panel, "quickWifiPower"));
        verify(!findChild(panel, "controlDetailsScroll"));
        const scroll = findChild(panel, "controlPageScroll");
        verify(scroll.contentHeight <= scroll.height);
        const tile = findChild(panel, "controlWifi");
        const beforeRequest = JSON.stringify(NacreControlActions.lastRequest);
        tile.clicked();
        compare(JSON.stringify(NacreControlActions.lastRequest), beforeRequest);
        compare(panel.section, "network");
        wait(30);
        verify(!!findChild(panel, "quickWifiPower"));
        verify(!findChild(panel, "controlVolume"));
        scene.selectedSection = "bluetooth";
        wait(30);
        verify(!findChild(panel, "quickWifiPower"));
        verify(!!findChild(panel, "quickBluetoothPower"));
        scene.selectedSection = "home";
        wait(30);
        verify(!findChild(panel, "quickBluetoothPower"));
        scene.height = 480;
        scene.width = 280;
        wait(30);
        verify(panel.width <= 280 && panel.height <= 480);
        verify(scroll.y + scroll.height <= panel.height);
        panel.visibility = false;
        wait(350);
        compare(panel.width, 0);
        verify(!findChild(panel, "controlVolume"));
        compare(findChild(panel, "controlBodyLoader").item, null);
    }
    function test_wifi_password_stays_in_panel_and_clears_on_submit() {
        const popup = createTemporaryObject(network, test);
        verify(popup.activate(popup.nearby.find(point => point.ssid === "Guest")));
        wait(0);
        const field = findChild(popup, "controlWifiPassword");
        verify(!!popup.pendingNetwork);
        field.text = "public-fixture-password";
        verify(popup.connectPending());
        compare(NacreControlActions.lastRequest[0], "wifi-inline");
        compare(NacreControlActions.lastRequest[1], "Guest");
        compare(field.text, "");
        popup.cancelPending();
        compare(popup.pendingNetwork, null);
    }
    function test_entry_modes_use_the_whole_viewport_and_release_after_closing() {
        const scene = createTemporaryObject(center, test), panel = scene.panel;
        wait(350);
        compare(panel.height, scene.height);
        scene.selectedSection = "network";
        wait(350);
        verify(!findChild(panel, "controlEdit").visible);
        verify(!findChild(panel, "controlPower").visible);
        const page = findChild(panel, "controlPageScroll");
        const networkPage = findChild(panel, "controlPageLoader").item;
        verify(networkPage.embedded);
        compare(page.height, panel.height - page.y - 18);
        panel.presentation = "dropdown";
        wait(350);
        verify(panel.height <= scene.height);
        compare(panel.width, panel.contentWidth);
        verify(findChild(panel, "controlFullPanel").visible);
        verify(!findChild(panel, "controlBack").visible);
        compare(panel.height, Math.min(scene.height, page.y + networkPage.implicitHeight + 18));
        panel.visibility = false;
        wait(80);
        compare(panel.width, panel.contentWidth);
        verify(panel.height < panel.contentHeight);
        wait(350);
        compare(panel.height, 0);
        compare(findChild(panel, "controlBodyLoader").item, null);
    }
    function test_embedded_network_scroll_reaches_its_last_row() {
        const list = createTemporaryObject(scrollingList, test, {
            embedded: true
        });
        compare(list.implicitHeight, 2200);
        verify(!list.interactive);
        const scene = createTemporaryObject(center, test), panel = scene.panel;
        scene.selectedSection = "network";
        panel.presentation = "dropdown";
        scene.height = 300;
        wait(350);
        const page = findChild(panel, "controlPageScroll");
        verify(page.contentHeight > page.height);
        page.contentY = page.contentHeight - page.height;
        verify(page.atYEnd);
        compare(page.y + page.height, panel.height - 18);
    }
    function test_dropdown_starts_below_the_bar_and_peek_handoff_keeps_its_size() {
        const scene = createTemporaryObject(center, test), panel = scene.panel;
        panel.visibility = false;
        wait(350);
        panel.presentation = "dropdown";
        compare(panel.height, 0);
        compare(panel.width, panel.contentWidth);
        panel.seedPreviewPeek(320, 100);
        compare(panel.presentedHeight, 100);
        panel.visibility = true;
        wait(40);
        verify(panel.height >= 100 && panel.height < panel.contentHeight);
        wait(350);
        compare(panel.height, panel.contentHeight);
    }
    function test_full_panel_button_keeps_pointer_and_keyboard_ownership() {
        const scene = createTemporaryObject(center, test), panel = scene.panel;
        panel.presentation = "dropdown";
        scene.selectedSection = "audio";
        wait(350);
        const button = findChild(panel, "controlFullPanelActivation");
        mouseClick(button, button.width / 2, button.height / 2);
        compare(panel.presentation, "sidebar");
        compare(panel.section, "home");
        verify(scene.sidebarPointerOwned);
        panel.presentation = "dropdown";
        scene.selectedSection = "audio";
        wait(350);
        const keyboardButton = findChild(panel, "controlFullPanelActivation");
        keyboardButton.forceActiveFocus(Qt.TabFocusReason);
        keyClick(Qt.Key_Return);
        compare(panel.presentation, "sidebar");
        verify(!scene.sidebarPointerOwned);
    }
    function test_calendar_selects_dates_and_resets_to_today() {
        NacreDashboardState.calendar = {
            events: [
                {
                    title: "Public event",
                    start: new Date(2026, 9, 9, 14, 0).getTime() / 1000,
                    time: "14:00"
                }
            ]
        };
        const scene = createTemporaryObject(center, test), panel = scene.panel;
        panel.presentation = "dropdown";
        scene.selectedSection = "calendar";
        wait(350);
        const month = findChild(panel, "controlCalendarMonth");
        verify(month !== null);
        compare(findChild(panel, "controlCalendarEventTime").text, "14:00");
        month.selectedDay = "2026-1-3";
        compare(month.dateForDay(month.selectedDay).getDate(), 3);
        findChild(panel, "controlCalendarToday").clicked();
        compare(month.selectedDay, month.todayKey);
        findChild(panel, "controlCalendarSettings").clicked();
        compare(NacreSettingsApp.lastPage, "panels/dashboard#calendarFile");
        verify(!findChild(panel, "controlEdit").visible);
        NacreDashboardState.calendar = {
            events: []
        };
    }
    function test_wifi_open_shows_cache_while_refreshing_and_never_starts_a_scan() {
        NacreNetwork.busy = true;
        NacreControlActions.lastRequest = [];
        const popup = createTemporaryObject(network, test);
        wait(30);
        verify(popup.nearby.length > 0);
        verify(popup.implicitHeight > 0);
        compare(NacreControlActions.lastRequest.length, 0);
        const scan = findChild(popup, "quickWifiScan");
        verify(scan.enabled);
        scan.clicked();
        compare(NacreControlActions.lastRequest.join("|"), "wifi-scan");
        NacreNetwork.busy = false;
    }
    function test_pointer_navigation_retains_exit_policy_and_keyboard_navigation_pins() {
        NacrePanelState.screens.test.osd = true;
        const scene = createTemporaryObject(center, test), panel = scene.panel;
        wait(40);
        const wifi = findChild(panel, "controlWifi");
        mouseClick(wifi, wifi.width / 2, wifi.height / 2);
        wait(30);
        compare(panel.section, "network");
        verify(NacrePanelState.screens.test.controlsPointerOwned);
        panel.navigate("home", false);
        wait(30);
        verify(!NacrePanelState.screens.test.controlsPointerOwned);
        const keyboardWifi = findChild(panel, "controlWifi");
        findChild(keyboardWifi, "controlWifiToggle").forceActiveFocus(Qt.TabFocusReason);
        keyClick(Qt.Key_Return);
        wait(30);
        compare(panel.section, "network");
        verify(!NacrePanelState.screens.test.controlsPointerOwned);
    }
    function test_control_center_optional_tiles_and_detail_back_focus() {
        const scene = createTemporaryObject(center, this), panel = scene.panel;
        panel.keyboardActive = true;
        scene.selectedSection = "edit";
        wait(30);
        NacreControlPreferences.toggle("mic");
        scene.selectedSection = "home";
        wait(30);
        verify(!!findChild(panel, "controlMic"));
        scene.selectedSection = "audio";
        wait(30);
        const back = findChild(panel, "controlBack");
        verify(!!back);
        back.clicked();
        compare(panel.section, "home");
        scene.selectedSection = "home";
        wait(30);
        for (const id of ["airplane", "vpn", "record", "mic"]) {
            NacreControlPreferences.tiles = NacreControlPreferences.defaults.concat([id]);
            wait(0);
            NacreControlPreferences.reset();
            wait(0);
        }
    }
    Component {
        id: pairing
        NacreBluetoothPairing {
            width: 352
            address: "02:00:00:00:00:01"
            deviceName: "Fixture headphones"
        }
    }
    function test_bluetooth_pairing_prompts_stay_inline_and_clear_credentials() {
        const prompt = createTemporaryObject(pairing, test);
        const agent = findChild(prompt, "controlBluetoothAgent");
        prompt.acceptOutput("Agent registered\n");
        compare(agent.writes[0], "pair 02:00:00:00:00:01\n");
        prompt.acceptOutput("Confirm passkey 123456 (yes/no): ");
        compare(prompt.mode, "confirm");
        prompt.respond("yes");
        compare(agent.writes[1], "yes\n");
        prompt.acceptOutput("Enter PIN code: ");
        compare(prompt.mode, "pin");
        const field = findChild(prompt, "controlBluetoothCode");
        field.text = "1234";
        prompt.respond(field.text);
        compare(field.text, "");
        compare(agent.writes[2], "1234\n");
        prompt.acceptOutput("Failed to pair: org.bluez.Error.AuthenticationRejected");
        compare(prompt.mode, "error");
        verify(prompt.finished);
        compare(prompt.buffer, "");
        compare(agent.signals[0], 15);
    }
    function test_tile_context_routes_do_not_toggle_and_only_device_tiles_show_arrows() {
        const scene = createTemporaryObject(center, test), panel = scene.panel;
        wait(60);
        const dnd = findChild(panel, "controlDnd"), wifi = findChild(panel, "controlWifi");
        verify(dnd && wifi);
        compare(dnd.hasDetails, false);
        compare(wifi.hasDetails, false);
        verify(wifi.navigation);
        const toggle = findChild(wifi, "controlWifiToggle");
        compare(toggle.radius, wifi.radius);
        compare(toggle.topRightRadius, wifi.radius);
        compare(toggle.drawFocusOutline, false);
        verify(!toggle.containsPoint(Qt.point(2, 2)));
        const request = NacreControlActions.lastRequest.join("|");
        mouseClick(wifi, 2, 2, Qt.LeftButton);
        compare(NacreControlActions.lastRequest.join("|"), request);
        toggle.forceActiveFocus(Qt.TabFocusReason);
        tryVerify(() => wifi.border.width === 2);
        const writes = DesktopSettings.writes.length;
        mouseClick(dnd, 20, 20, Qt.RightButton);
        compare(panel.section, "notifications");
        compare(DesktopSettings.writes.length, writes);
        scene.selectedSection = "home";
        wait(30);
        const awake = findChild(panel, "controlAwake");
        mousePress(awake, 20, 20, Qt.LeftButton);
        wait(650);
        compare(panel.section, "awake");
        mouseRelease(panel, 20, 20, Qt.LeftButton);
        compare(DesktopSettings.writes.length, writes);
    }
    function test_attached_surface_uses_shared_frame_and_power_stays_inline() {
        const scene = createTemporaryObject(center, test), panel = scene.panel;
        wait(40);
        const background = findChild(panel, "controlDetachedBackground");
        verify(!background.visible);
        compare(background.border.width, 0);
        verify(!findChild(panel, "controlHeaderTint"));
        panel.detached = true;
        verify(background.visible);
        findChild(panel, "controlPower").clicked();
        wait(30);
        compare(panel.section, "session");
        verify(!!findChild(panel, "controlSession-lock"));
        compare(DesktopActions.calls.length, 0);
        verify(!NacrePanelState.screens.test.session);
        scene.selectedSection = "home";
        wait(30);
        verify(!findChild(panel, "controlSession-lock"));
    }
    function test_radio_and_power_tiles_have_one_safe_navigation_target() {
        const scene = createTemporaryObject(center, test), panel = scene.panel;
        const before = JSON.stringify(NacreControlActions.lastRequest), profile = PowerProfiles.profile;
        for (const entry of [
            {
                id: "controlWifi",
                section: "network"
            },
            {
                id: "controlBluetooth",
                section: "bluetooth"
            },
            {
                id: "controlPowerMode",
                section: "battery"
            }
        ]) {
            scene.selectedSection = "home";
            wait(40);
            const tile = findChild(panel, entry.id);
            verify(tile.navigation && !tile.hasDetails);
            const input = findChild(tile, entry.id + "Toggle");
            compare(input.width, tile.width);
            mouseClick(tile, tile.width - 14, tile.height / 2, Qt.LeftButton);
            wait(30);
            compare(panel.section, entry.section);
            compare(JSON.stringify(NacreControlActions.lastRequest), before);
            compare(PowerProfiles.profile, profile);
        }
    }
    function test_detected_input_sliders_are_prominent_and_added_beside_related_levels() {
        NacreControlPreferences.toggle("mic-volume");
        NacreControlPreferences.toggle("keyboard");
        const scene = createTemporaryObject(center, test), panel = scene.panel;
        scene.selectedSection = "edit";
        wait(40);
        const mic = findChild(panel, "controlAdd-mic-volume"), keyboard = findChild(panel, "controlAdd-keyboard");
        verify(mic && keyboard);
        const page = findChild(panel, "controlPageScroll");
        verify(mic.mapToItem(page, 0, 0).y < page.height);
        verify(keyboard.mapToItem(page, 0, 0).y < page.height);
        mic.clicked();
        wait(30);
        findChild(panel, "controlAdd-keyboard").clicked();
        wait(30);
        compare(NacreControlPreferences.tiles.indexOf("mic-volume"), NacreControlPreferences.tiles.indexOf("volume") + 1);
        compare(NacreControlPreferences.tiles.indexOf("keyboard"), NacreControlPreferences.tiles.indexOf("brightness") + 1);
        scene.selectedSection = "home";
        wait(40);
        verify(findChild(panel, "controlMicrophone").visible);
        verify(findChild(panel, "controlKeyboard").visible);
        NacreControlPreferences.toggle("mic-volume");
        NacreControlPreferences.toggle("keyboard");
        NacreKeyboardLight.available = false;
        NacreAudio.micAvailable = false;
        scene.selectedSection = "edit";
        wait(30);
        verify(!findChild(panel, "controlAdd-mic-volume"));
        verify(!findChild(panel, "controlAdd-keyboard"));
        NacreKeyboardLight.available = true;
        NacreAudio.micAvailable = true;
    }
    function test_attached_control_center_fills_frame_and_resizes_on_every_page() {
        const scene = createTemporaryObject(center, test), panel = scene.panel;
        for (const section of ["home", "audio", "network", "bluetooth", "battery", "session", "notifications"]) {
            scene.selectedSection = section;
            wait(250);
            compare(panel.height, scene.height);
        }
        scene.height = 540;
        tryCompare(panel, "height", 540);
        scene.height = 920;
        tryCompare(panel, "height", 920);
    }
    function test_all_detected_sliders_are_default_and_follow_device_availability() {
        const scene = createTemporaryObject(center, test), panel = scene.panel;
        wait(40);
        for (const name of ["Volume", "Microphone", "Brightness", "Keyboard"])
            verify(findChild(panel, "control" + name).visible);
        NacreAudio.available = false;
        NacreAudio.micAvailable = false;
        NacreBrightness.device.available = false;
        NacreKeyboardLight.available = false;
        wait(40);
        for (const name of ["Volume", "Microphone", "Brightness", "Keyboard"])
            verify(!findChild(panel, "control" + name));
        NacreAudio.micAvailable = true;
        wait(40);
        verify(findChild(panel, "controlMicrophone").visible);
        verify(!findChild(panel, "controlVolume"));
        NacreAudio.available = true;
        NacreBrightness.device.available = true;
        NacreKeyboardLight.available = true;
    }
    function test_slider_default_rollout_preserves_order_and_saved_removals() {
        const prefs = NacreControlPreferences;
        verify(prefs.receive(JSON.stringify({
            version: 2,
            tiles: ["bluetooth", "volume", "network", "brightness", "picker"]
        })));
        compare(prefs.tiles.join("|"), "bluetooth|volume|mic-volume|network|brightness|keyboard|picker");
        prefs.toggle("mic-volume");
        prefs.toggle("keyboard");
        const saved = findChild(prefs, "controlPreferencesStore").saved;
        compare(JSON.parse(saved).defaultsRevision, 1);
        verify(prefs.receive(saved));
        compare(prefs.tiles.join("|"), "bluetooth|volume|network|brightness|picker");
        verify(prefs.receive(JSON.stringify({
            version: 2,
            defaultsRevision: 2,
            tiles: ["volume"]
        })));
        prefs.save(prefs.tiles);
        compare(JSON.parse(findChild(prefs, "controlPreferencesStore").saved).defaultsRevision, 2);
        prefs.defaultsRevision = 1;
    }
    function test_profiles_are_navigation_and_power_view_fits_its_actions() {
        const scene = createTemporaryObject(center, test), panel = scene.panel;
        panel.presentation = "dropdown";
        wait(40);
        const profile = PowerProfiles.profile;
        const mode = findChild(panel, "controlPowerMode");
        verify(mode && !mode.selected);
        mode.clicked();
        wait(30);
        compare(panel.section, "battery");
        compare(PowerProfiles.profile, profile);
        scene.selectedSection = "session";
        wait(230);
        compare(panel.height, scene.height);
        const page = findChild(panel, "controlPageLoader").item;
        page.choose("restart");
        wait(230);
        compare(panel.height, scene.height);
        compare(AppLaunch.calls.length, 0);
    }
    function test_wired_desktop_hides_unsupported_levels_and_has_no_media() {
        NacreNetwork.wiredConnected = true;
        NacreNetwork.wifiAvailable = false;
        NacreBrightness.device.available = false;
        NacreKeyboardLight.available = false;
        NacrePlayers.active = ({
                trackTitle: "Public demo track",
                trackArtist: "Demo artist"
            });
        const scene = createTemporaryObject(center, test), panel = scene.panel;
        wait(250);
        const network = findChild(panel, "controlWifi");
        compare(network.label, "Ethernet");
        compare(network.icon, "lan");
        verify(network.selected && network.available);
        const brightness = findChild(panel, "controlBrightnessRow");
        verify(!brightness || !brightness.visible);
        compare(panel.height, scene.height);
        verify(!containsText(panel, "Public demo track"));
        const request = NacreControlActions.lastRequest;
        network.clicked();
        wait(30);
        compare(panel.section, "network");
        compare(NacreControlActions.lastRequest, request);
        verify(findChild(panel, "controlWiredConnection").visible);
        scene.selectedSection = "battery";
        wait(30);
        verify(!findChild(panel, "controlKeyboardBrightness").visible);
        NacreKeyboardLight.available = true;
        verify(findChild(panel, "controlKeyboardBrightness").visible);
        scene.selectedSection = "home";
        wait(250);
        NacreNetwork.wiredConnected = false;
        NacreNetwork.wifiAvailable = true;
        compare(findChild(panel, "controlWifi").label, "Wi-Fi");
        const compactHeight = panel.height;
        NacreBrightness.device.available = true;
        wait(250);
        verify(findChild(panel, "controlBrightnessRow").visible);
        compare(panel.height, compactHeight);
    }
    function containsText(item, text) {
        return item.text === text || (item.children || []).some(child => containsText(child, text));
    }
    function test_editor_filtered_layout_keeps_one_heading_and_shared_keyboard_order() {
        const view = createTemporaryObject(editor, test, {
            section: "layout",
            settingsRows: true,
            showHeadings: false,
            showHint: false,
            showReset: false
        });
        wait(30);
        verify(!findChild(view, "controlEditHint").visible);
        verify(!findChild(view, "controlEditLayoutHeading").visible);
        verify(!findChild(view, "controlLayoutReset").visible);
        verify(!findChild(view, "controlAdd-mic-volume"));
        verify(!findChild(view, "controlAdd-screenshot"));
        const first = NacreControlPreferences.tiles[0], next = findChild(view, "controlMoveDown-" + first), previous = findChild(view, "controlMoveUp-" + first);
        verify(!previous.enabled && previous.opacity < 1);
        next.focusControl();
        keyClick(Qt.Key_Space);
        wait(30);
        compare(NacreControlPreferences.tiles[1], first);
        const remove = findChild(view, "controlRemove-network");
        remove.focusControl();
        keyClick(Qt.Key_Space);
        wait(30);
        verify(!NacreControlPreferences.tiles.includes("network"));
    }
    function test_editor_detected_and_available_rows_filter_support_and_inherit_disabled() {
        NacreControlPreferences.toggle("mic-volume");
        NacreControlPreferences.toggle("keyboard");
        const view = createTemporaryObject(editor, test, {
            section: "detected",
            settingsRows: true,
            showHeadings: false,
            showHint: false,
            showReset: false
        });
        wait(30);
        verify(!findChild(view, "controlRemove-network"));
        verify(!findChild(view, "controlAdd-screenshot"));
        const mic = findChild(view, "controlAdd-mic-volume"), add = findChild(view, "controlAdd-mic-volumeButton");
        verify(mic && add);
        fuzzyCompare(add.mapToItem(view, 0, 0).x + add.width, view.width, .5);
        mic.focusControl();
        keyClick(Qt.Key_Space);
        wait(30);
        compare(NacreControlPreferences.tiles.indexOf("mic-volume"), NacreControlPreferences.tiles.indexOf("volume") + 1);
        verify(!findChild(view, "controlAdd-mic-volume"));
        const keyboard = findChild(view, "controlAdd-keyboardButton"), row = findChild(view, "controlAdd-keyboardRow"), before = JSON.stringify(NacreControlPreferences.tiles);
        view.enabled = false;
        verify(!row.enabled && row.opacity < 1 && !keyboard.enabled);
        mouseClick(keyboard, keyboard.width / 2, keyboard.height / 2);
        compare(JSON.stringify(NacreControlPreferences.tiles), before);
        view.enabled = true;
        view.section = "available";
        wait(30);
        verify(!findChild(view, "controlAdd-keyboard"));
        verify(!!findChild(view, "controlAdd-screenshot"));
        NacreControlTools.data = Object.assign({}, NacreControlTools.data, {
            screenshot: false,
            colorPicker: false
        });
        wait(30);
        verify(!findChild(view, "controlAdd-screenshot"));
        verify(!findChild(view, "controlAdd-picker"));
    }
    function test_editor_removes_reorders_adds_and_resets_all_controls() {
        const scene = createTemporaryObject(center, test), panel = scene.panel;
        scene.selectedSection = "edit";
        wait(40);
        verify(!!findChild(panel, "controlRemove-network"));
        findChild(panel, "controlRemove-network").focusControl();
        keyClick(Qt.Key_Space);
        wait(30);
        verify(!NacreControlPreferences.tiles.includes("network"));
        verify(!!findChild(panel, "controlAdd-network"));
        findChild(panel, "controlAdd-network").clicked();
        wait(30);
        compare(NacreControlPreferences.tiles[NacreControlPreferences.tiles.length - 1], "network");
        findChild(panel, "controlMoveUp-network").clicked();
        wait(30);
        compare(NacreControlPreferences.tiles[NacreControlPreferences.tiles.length - 2], "network");
        findChild(panel, "controlRemove-volume").clicked();
        wait(30);
        verify(!NacreControlPreferences.tiles.includes("volume"));
        findChild(panel, "controlAdd-screenshot").clicked();
        wait(30);
        scene.selectedSection = "home";
        wait(40);
        verify(!findChild(panel, "controlVolume"));
        verify(findChild(panel, "controlMicrophone").visible);
        const writes = NacreAudio.writes;
        findChild(panel, "controlMicrophone").value = .55;
        findChild(panel, "controlMicrophone").moved();
        compare(NacreAudio.writes, writes + 1);
        const captures = NacreControlTools.captures.length;
        findChild(panel, "controlAction-screenshot").clicked();
        compare(NacreControlTools.captures.length, captures + 1);
        compare(NacreControlTools.captures[NacreControlTools.captures.length - 1], "screenshot");
        scene.selectedSection = "edit";
        wait(30);
        findChild(panel, "controlLayoutReset").clicked();
        compare(JSON.stringify(NacreControlPreferences.tiles), JSON.stringify(NacreControlPreferences.defaults));
    }
    function test_saved_layout_migration_validation_empty_layout_and_unsupported_features() {
        const prefs = NacreControlPreferences;
        verify(prefs.receive(JSON.stringify({
            version: 1,
            optionalTiles: ["mic", "unknown", "mic"]
        })));
        compare(prefs.tiles.length, prefs.defaults.length + 1);
        compare(prefs.tiles[prefs.tiles.length - 1], "mic");
        verify(prefs.receive(JSON.stringify({
            version: 2,
            tiles: ["bluetooth", "network", "bluetooth", "unknown"]
        })));
        compare(prefs.tiles.join("|"), "bluetooth|network");
        verify(!prefs.receive("malformed"));
        compare(prefs.tiles.join("|"), "bluetooth|network");
        verify(!prefs.move("network", -2));
        verify(prefs.move("network", -1));
        compare(prefs.tiles.join("|"), "network|bluetooth");
        compare(JSON.parse(findChild(prefs, "controlPreferencesStore").saved).version, 2);
        verify(prefs.receive(JSON.stringify({
            version: 2,
            tiles: []
        })));
        compare(prefs.tiles.length, 0);
        prefs.reset();
        NacreControlTools.data = Object.assign({}, NacreControlTools.data, {
            nightLightSupported: false,
            screenshot: false,
            colorPicker: false
        });
        const scene = createTemporaryObject(center, test), panel = scene.panel;
        wait(40);
        verify(!findChild(panel, "controlNightLight"));
        scene.selectedSection = "edit";
        wait(30);
        verify(!findChild(panel, "controlRemove-night"));
        verify(!findChild(panel, "controlAdd-screenshot"));
        verify(!findChild(panel, "controlAdd-picker"));
        verify(prefs.tiles.includes("night"));
        NacreControlTools.data = Object.assign({}, NacreControlTools.data, {
            nightLightSupported: true
        });
        scene.selectedSection = "home";
        wait(30);
        verify(!!findChild(panel, "controlNightLight"));
    }
    Component {
        id: sessionControls
        NacreControlSession {
            width: 388
        }
    }
    function test_session_confirmation_cancel_and_inactive_guards() {
        const page = createTemporaryObject(sessionControls, test);
        verify(page.choose("restart"));
        compare(AppLaunch.calls.length, 0);
        page.pendingKey = "";
        verify(!page.confirm());
        verify(!page.choose("unknown"));
        verify(page.choose("poweroff"));
        page.active = false;
        compare(page.pendingKey, "");
        verify(!page.confirm());
        verify(!page.choose("lock"));
        page.active = true;
        verify(page.choose("restart"));
        verify(page.confirm());
        compare(AppLaunch.calls.length, 1);
        compare(AppLaunch.calls[0].join("|"), "systemctl|reboot");
        verify(!page.confirm());
        verify(page.choose("logout"));
        verify(page.confirm());
        compare(NacreCompositor.logoutCount, 1);
        verify(page.choose("lock"));
        compare(DesktopActions.calls[0], "lock");
    }
    function test_session_shortcut_routes_into_control_center() {
        verify(NacrePanelState.toggleSession());
        compare(NacrePanelState.screens.test.controlSection, "session");
        verify(NacrePanelState.screens.test.osd);
        verify(!NacrePanelState.screens.test.session);
        verify(NacrePanelState.toggleSession());
        verify(!NacrePanelState.screens.test.osd);
    }
    function test_input_peak_monitor_stops_when_sound_page_inactive() {
        const popup = createTemporaryObject(sound, test);
        const peak = findChild(popup, "controlInputPeak");
        verify(peak.enabled);
        popup.active = false;
        compare(peak.enabled, false);
        popup.active = true;
        NacreAudio.micMuted = true;
        compare(peak.enabled, false);
        NacreAudio.micMuted = false;
    }
    Component {
        id: savedProfiles
        NacreWifiSaved {}
    }
    Component {
        id: savedQueryStarts
        SignalSpy {
            signalName: "started"
        }
    }
    function test_saved_wifi_queries_ssids_without_helper_changes() {
        const saved = createTemporaryObject(savedProfiles, test);
        const worker = findChild(saved, "controlSavedWifiReader");
        compare(worker.command.slice(-2).join("|"), "connection|show");
        worker.running = false;
        worker.stdout.text = "11111111-1111-4111-8111-111111111111:802-11-wireless\n22222222-2222-4222-8222-222222222222:ethernet";
        worker.stdout.streamFinished();
        worker.exited(0, 0);
        tryVerify(() => worker.running, 500);
        compare(worker.command.slice(-3).join("|"), "show|uuid|11111111-1111-4111-8111-111111111111");
        worker.running = false;
        worker.stdout.text = "Saved: Studio\n";
        worker.stdout.streamFinished();
        worker.exited(0, 0);
        compare(saved.ssids.join("|"), "Saved: Studio");
        compare(saved.ready, true);
    }
    function test_closing_wifi_cancels_its_queued_profile_query() {
        const saved = createTemporaryObject(savedProfiles, test);
        wait(0);
        const worker = findChild(saved, "controlSavedWifiReader");
        const starts = createTemporaryObject(savedQueryStarts, test, {
            target: worker
        });
        worker.running = false;
        worker.stdout.text = "11111111-1111-4111-8111-111111111111:802-11-wireless";
        worker.stdout.streamFinished();
        worker.exited(0, 0);
        compare(worker.starts, 1);
        saved.enabled = false;
        saved.destroy();
        wait(50);
        compare(starts.count, 0);
    }
    function test_quick_list_wheel_speed_tail_clamp_and_reduce_motion() {
        const actionsBefore = JSON.stringify(NacreControlActions.lastRequest);
        DesktopSettings.data = {
            animations: true
        };
        const view = createTemporaryObject(scrollingList, test);
        const scroll = findChild(view, "quickListScroll");
        scroll.scrollBy(240, true);
        wait(30);
        verify(view.contentY > 0 && view.contentY < 240);
        wait(240);
        compare(Math.round(view.contentY), 240);
        scroll.pixelScroll(50);
        wait(20);
        scroll.pixelScroll(50);
        const releasePosition = view.contentY;
        wait(140);
        verify(view.contentY > releasePosition);
        scroll.cancel();
        const parked = view.contentY;
        DesktopSettings.data = {
            animations: false
        };
        compare(view.contentY, parked);
        scroll.scrollBy(200, true);
        compare(view.contentY, parked + 200);
        scroll.scrollBy(10000, true);
        compare(view.contentY, view.contentHeight - view.height);
        view.contentItem.children[0].implicitHeight = 120;
        wait(0);
        compare(view.contentY, 0);
        compare(JSON.stringify(NacreControlActions.lastRequest), actionsBefore);
    }
    function test_sound_controls_and_settings_link() {
        const popup = createTemporaryObject(sound, test);
        verify(popup.implicitHeight > 100 && popup.implicitHeight < 600);
        const volume = findChild(popup, "quickOutputVolume");
        volume.value = 0.5;
        volume.moved();
        compare(NacreAudio.volume, 0.5);
        findChild(popup, "quickMicMute").clicked();
        compare(NacreAudio.micMuted, true);
        verify(!findChild(popup, "quickSettingsLink"));
        NacrePanelState.openDeviceSettings("audio");
        compare(NacrePanelState.screens.test.controlSection, "audio");
        verify(NacrePanelState.screens.test.osd);
        verify(!NacrePanelState.screens.test.dashboard);
        verify(!NacrePanelState.panels.test.popouts.hasCurrent);
        verify(!NacrePanelState.panels.test.popouts.pinned);
    }

    function test_connected_ap_updates_when_same_name_roams() {
        compare(groupedAPs.length, 1);
        compare(groupedAPs[0], secondAP);
        firstAP.active = true;
        secondAP.active = false;
        compare(groupedAPs[0], firstAP);
        firstAP.active = false;
        firstAP.strength = 10;
        compare(groupedAPs[0], secondAP);
        firstAP.strength = 90;
        secondAP.active = true;
    }

    function test_no_battery_popup_explains_unavailable() {
        const prior = UPower.displayDevice;
        UPower.displayDevice = {
            ready: true,
            isLaptopBattery: false,
            isPresent: false,
            percentage: 0
        };
        const popup = createTemporaryObject(assembly, test);
        popup.currentName = "battery";
        popup.hasCurrent = true;
        verify(popup.currentItem !== null);
        verify(!popup.currentItem.available);
        compare(popup.currentItem.percent, -1);
        compare(popup.currentItem.estimate, "Battery unavailable");
        popup.hasCurrent = false;
        UPower.displayDevice = prior;
    }
    function test_absent_radios_disable_actions_without_errors() {
        NacreNetwork.wifiAvailable = false;
        NacreNetwork.wifiEnabled = false;
        NacreBluetooth.available = false;
        NacreBluetooth.powered = false;
        const wifi = createTemporaryObject(network, test);
        const blue = createTemporaryObject(bluetooth, test);
        verify(!findChild(wifi, "quickWifiPower").enabled);
        verify(!findChild(blue, "quickBluetoothPower").enabled);
        NacreControlActions.lastRequest = [];
        findChild(wifi, "quickWifiPower").clicked();
        findChild(blue, "quickBluetoothPower").clicked();
        compare(NacreControlActions.lastRequest.length, 0);
        verify(!wifi.activate(wifi.nearby[0]));
        verify(!blue.activate(blue.known[0]));
        NacreNetwork.wifiAvailable = true;
        NacreNetwork.wifiEnabled = true;
        NacreBluetooth.available = true;
        NacreBluetooth.powered = true;
    }
    function test_wifi_deduplicates_and_opens_network_settings() {
        const popup = createTemporaryObject(network, test);
        compare(popup.nearby.length, 2);
        compare(popup.nearby[0].ssid, "ab");
        verify(popup.nearby[0].active);
        compare(popup.nearby[0].strength, 30);
        const disconnect = findChild(popup, "connectedWifiAction");
        verify(disconnect);
        compare(disconnect.text, "Disconnect");
        disconnect.clicked();
        compare(NacreControlActions.lastRequest.join("|"), "wifi-disconnect|wlan0");
        findChild(popup, "availableWifiAction").clicked();
        compare(popup.pendingNetwork.ssid, "Guest");
        verify(popup.connectPending());
        compare(NacreControlActions.lastRequest[0], "wifi-inline");
        findChild(popup, "quickWifiPower").clicked();
        compare(NacreControlActions.lastRequest.join("|"), "wifi-radio|off");
        const scan = findChild(popup, "quickWifiScan");
        mouseClick(scan, scan.width / 2, scan.height / 2);
        compare(NacreControlActions.lastRequest[0], "wifi-scan");
        verify(!findChild(popup, "quickSettingsLink"));
        NacrePanelState.openDeviceSettings("network");
        compare(NacrePanelState.screens.test.controlSection, "network");
    }

    function test_bluetooth_shows_known_devices_and_links_settings() {
        const popup = createTemporaryObject(bluetooth, test);
        compare(popup.known.length, 2);
        compare(popup.known[0].name, "Headphones");
        findChild(popup, "quickBluetoothPower").clicked();
        compare(NacreControlActions.lastRequest.join("|"), "bluetooth-power|off");
        verify(!findChild(popup, "quickSettingsLink"));
        NacrePanelState.openDeviceSettings("bluetooth");
        compare(NacrePanelState.screens.test.controlSection, "bluetooth");
    }

    function test_display_never_changes_devices_or_history_and_rejects_stale_rows() {
        const wifi = createTemporaryObject(network, test);
        const bt = createTemporaryObject(bluetooth, test);
        const audio = createTemporaryObject(sound, test);
        const previous = NacreControlActions.lastRequest.join("|");
        verify(!wifi.activate({
            bssid: "AA:BB:CC:DD:EE:99",
            ssid: "Missing",
            active: false
        }));
        verify(!bt.activate({
            address: "AA:BB:CC:DD:EE:99",
            connected: false
        }));
        compare(NacreControlActions.lastRequest.join("|"), previous);
        NacreControlActions.busy = true;
        verify(!wifi.activate(wifi.nearby[0]));
        verify(!bt.activate(bt.known[0]));
        compare(NacreControlActions.lastRequest.join("|"), previous);
        NacreControlActions.busy = false;
        verify(bt.activate(bt.known[0]));
        compare(NacreControlActions.lastRequest.join("|"), "bluetooth-disconnect|AA:BB:CC:DD:EE:FF");
        verify(!audio.chooseOutput({
            ready: true,
            audio: {},
            isSink: true
        }));
    }
    function test_audio_backend_changes_never_write_and_slider_user_action_does() {
        const audio = createTemporaryObject(sound, test);
        compare(NacreAudio.writes, 0);
        NacreAudio.volume = .42;
        compare(findChild(audio, "quickOutputVolume").value, .42);
        compare(NacreAudio.writes, 0);
        const slider = findChild(audio, "quickOutputVolume");
        slider.value = .6;
        slider.moved();
        compare(NacreAudio.writes, 1);
        compare(NacreAudio.volume, .6);
    }
    function test_mixer_and_input_selection_guard_removed_nodes() {
        const previous = Pipewire.nodes;
        const input = {
            id: 20,
            ready: true,
            isSink: false,
            isStream: false,
            description: "Fixture microphone",
            audio: {
                volume: .5,
                muted: false
            }
        };
        const stream = {
            id: 21,
            ready: true,
            isSink: false,
            isStream: true,
            description: "Fixture music",
            audio: {
                volume: .7,
                muted: false
            }
        };
        Pipewire.nodes = {
            values: [input, stream]
        };
        const popup = createTemporaryObject(sound, test);
        compare(popup.inputs.length, 1);
        compare(popup.streams.length, 1);
        verify(popup.chooseInput(input));
        compare(Pipewire.preferredDefaultAudioSource, input);
        verify(popup.setStream(stream, 1.4));
        compare(stream.audio.volume, 1);
        verify(popup.muteStream(stream));
        compare(stream.audio.muted, true);
        Pipewire.nodes = {
            values: []
        };
        verify(!popup.setStream(stream, .2));
        verify(!popup.chooseInput(input));
        compare(stream.audio.volume, 1);
        Pipewire.nodes = previous;
    }
    function test_every_route_and_closing_retention_unload_reversal() {
        DesktopSettings.data = {
            animations: true
        };
        const popup = createTemporaryObject(assembly, test);
        for (const name of ["audio", "network", "bluetooth", "notifications", "battery", "calendar"]) {
            popup.currentName = name;
            popup.hasCurrent = true;
            tryVerify(() => popup.currentItem !== null, 500);
            verify(popup.targetWidth >= 150 && popup.targetWidth <= 670);
            tryVerify(() => popup.height > 0, 300);
            verify(popup.currentItem.enabled);
            popup.hasCurrent = false;
            verify(!popup.enabled);
            verify(!popup.pinned);
            verify(popup.currentItem !== null);
            const width = popup.width;
            wait(40);
            compare(popup.width, width);
            popup.hasCurrent = true;
            wait(220);
            verify(popup.currentItem !== null && popup.height > 0);
            popup.hasCurrent = false;
            tryCompare(popup, "height", 0, 400);
            tryCompare(popup, "currentItem", null, 400);
        }
        popup.currentName = "toString";
        popup.hasCurrent = true;
        verify(!popup.hasCurrent);
        compare(popup.currentItem, null);
    }
    function test_bar_dropdown_uses_shared_popup_motion_and_unloads() {
        const popup = createTemporaryObject(assembly, test);
        popup.currentName = "peek";
        for (const kind of ["audio", "network", "bluetooth", "battery", "updates", "notifications"]) {
            popup.peekKind = kind;
            popup.hasCurrent = true;
            tryVerify(() => popup.currentItem !== null, 500);
            compare(popup.currentItem.kind, kind);
            verify(popup.currentItem.summary.length > 0);
            tryVerify(() => popup.height > 0, 300);
            verify(!popup.pinned);
            popup.hasCurrent = false;
            tryCompare(popup, "height", 0, 400);
            tryCompare(popup, "currentItem", null, 400);
        }
    }
    function test_overview_shows_two_latest_notices_and_opens_full_history() {
        NacreNotifs.retained = [
            {
                summary: "First",
                expanded: false
            },
            {
                summary: "Second",
                expanded: false
            },
            {
                summary: "Newest",
                expanded: false
            }
        ];
        NacreNotifs.clears = 0;
        const host = createTemporaryObject(center, test), panel = host.panel;
        wait(40);
        const overview = findChild(panel, "controlPageLoader").item;
        const loader = findChild(overview, "controlNotificationLoader");
        verify(loader.item !== null);
        compare(loader.item.preview.length, 2);
        compare(loader.item.preview[0].summary, "Newest");
        compare(NacreNotifs.clears, 0);
        overview.active = false;
        wait(20);
        compare(loader.item, null);
        overview.active = true;
        wait(20);
        findChild(loader.item, "controlAllNotifications").clicked();
        compare(panel.section, "notifications");
        NacreNotifs.retained = [];
    }
    function test_notification_history_fills_resizes_and_reaches_last_card_without_outer_scroll() {
        NacreNotifs.retained = Array.from({
            length: 18
        }, (_, index) => ({
                    summary: "Public message " + index,
                    expanded: false
                }));
        const host = createTemporaryObject(center, test), panel = host.panel;
        panel.navigate("notifications");
        wait(40);
        const page = findChild(panel, "controlPageScroll"), history = findChild(panel, "controlPageLoader").item, list = findChild(history, "notificationHistoryList"), bottom = findChild(history, "notificationHistoryBottomFade");
        compare(history.entries[0].summary, "Public message 17");
        verify(!page.interactive);
        fuzzyCompare(list.y + list.height, page.height, .5);
        verify(list.contentHeight > list.height && bottom.visible);
        verify(!findChild(panel, "controlPageWheel").enabled);
        mouseWheel(list, list.width / 2, 40, 0, -120);
        tryVerify(() => list.contentY > 0, 1000);
        tryCompare(findChild(list, "quickListScroll").motion, "running", false, 1000);
        compare(page.contentY, 0);
        panel.detached = true;
        host.height = 540;
        wait(40);
        fuzzyCompare(history.availableHeight, page.height, .5);
        fuzzyCompare(list.y + list.height, page.height, .5);
        list.contentY = list.contentHeight - list.height;
        wait(20);
        verify(list.atYEnd && !bottom.visible);
        compare(page.contentY, 0);
        NacreNotifs.retained = [];
    }
    function test_notification_control_page_is_lazy_and_reuses_history() {
        NacreNotifs.retained = [
            {
                expanded: false
            }
        ];
        NacreNotifs.clears = 0;
        const host = createTemporaryObject(center, test), panel = host.panel;
        panel.navigate("notifications");
        wait(40);
        const page = findChild(panel, "controlPageLoader");
        verify(page.item !== null);
        compare(page.item.showHeading, false);
        compare(NacreNotifs.clears, 0);
        verify(findChild(page.item, "quickClearHistory").enabled);
        verify(findChild(page.item, "quickDnd") !== null);
        panel.navigate("home");
        wait(40);
        verify(findChild(page.item, "quickClearHistory") !== null);
        NacreControlPreferences.save(["notifications"]);
        wait(40);
        const tile = findChild(panel, "controlAction-notifications");
        verify(tile && tile.navigation && !tile.hasDetails);
        mouseClick(tile, 20, 20);
        compare(panel.section, "notifications");
        NacreNotifs.retained = [];
    }
    function test_history_user_only_controls_and_pinned_escape() {
        NacreNotifs.retained = [
            {
                expanded: false
            }
        ];
        NacreNotifs.clears = 0;
        frameEscapes = 0;
        DesktopSettings.writes = [];
        const popup = createTemporaryObject(assembly, test);
        activePopup = popup;
        popup.currentName = "notifications";
        popup.hasCurrent = true;
        popup.pinned = true;
        tryVerify(() => popup.currentItem !== null && popup.focus, 400);
        compare(NacreNotifs.clears, 0);
        compare(DesktopSettings.writes.length, 0);
        findChild(popup.currentItem, "quickDnd").clicked();
        compare(DesktopSettings.writes[0].join("|"), "dnd|true");
        findChild(popup.currentItem, "quickClearHistory").clicked();
        compare(NacreNotifs.clears, 1);
        keyClick(Qt.Key_Escape);
        compare(frameEscapes, 1);
        verify(!popup.hasCurrent);
        verify(!popup.pinned);
        NacreNotifs.retained = [];
        activePopup = null;
    }
    function test_reduced_motion_unloads_without_lingering_view() {
        DesktopSettings.data = {
            animations: false
        };
        const popup = createTemporaryObject(assembly, test);
        popup.currentName = "audio";
        popup.hasCurrent = true;
        verify(popup.currentItem !== null);
        popup.hasCurrent = false;
        compare(popup.height, 0);
        compare(popup.currentItem, null);
    }
    Component {
        id: assembly
        NacrePopupPanel {
            screen: ({
                    name: "test",
                    height: 1000
                })
        }
    }

    name: "QuickDeviceControls"
    width: 700
    height: 700
    visible: true
    when: windowShown

    Component {
        id: sound

        AudioPopup {}
    }

    Component {
        id: network

        NetworkPopup {}
    }

    Component {
        id: bluetooth

        BluetoothPopup {}
    }

    QtObject {
        id: firstAP

        property string ssid: "Shared network"
        property bool active: false
        property int strength: 90
    }

    QtObject {
        id: secondAP

        property string ssid: "Shared network"
        property bool active: true
        property int strength: 30
    }
}
