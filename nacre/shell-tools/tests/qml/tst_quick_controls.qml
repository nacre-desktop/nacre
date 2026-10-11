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
        DeviceActions.busy = false;
        NacreAudio.writes = 0;
        DesktopSettings.writes = [];
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
            NacreControlCenter {
                id: panel
                width: implicitWidth
                height: implicitHeight
                screen: ({
                        name: "test"
                    })
                section: parent.selectedSection
                onSectionRequested: section => parent.selectedSection = section
                visibility: true
            }
        }
    }
    function test_control_center_open_is_readonly_scrolls_are_separate_and_small_bounds() {
        NacreAudio.writes = 0;
        NacreKeyboardLight.writes = [];
        NacreBrightness.device.writes = [];
        NacreControlTools.writes = 0;
        const scene = createTemporaryObject(center, this), panel = scene.panel;
        wait(50);
        verify(!panel.activeFocus);
        panel.keyboardActive = true;
        tryCompare(panel, "activeFocus", true);
        compare(NacreAudio.writes, 0);
        compare(NacreKeyboardLight.writes.length, 0);
        compare(NacreBrightness.device.writes.length, 0);
        compare(NacreControlTools.writes, 0);
        const volume = findChild(panel, "controlVolume");
        verify(!!volume);
        verify(volume.width > 200);
        volume.value = .8;
        volume.moved();
        compare(NacreAudio.writes, 1);
        const notifications = findChild(panel, "controlDetailsScroll"), controls = findChild(panel, "controlPrimaryScroll");
        verify(notifications.y >= controls.y + controls.height);
        verify(notifications.height > 80);
        controls.contentY = 10;
        compare(notifications.contentY, 0);
        findChild(panel, "controlWifi").clicked();
        compare(panel.section, "network");
        scene.selectedSection = "bluetooth";
        compare(panel.section, "bluetooth");
        scene.selectedSection = "network";
        wait(30);
        verify(findChild(panel, "quickWifiPower") !== null);
        scene.height = 480;
        scene.width = 280;
        wait(30);
        verify(panel.width <= 280 && panel.height <= 480);
        verify(notifications.y + notifications.height <= panel.height);
        verify(controls.height > 100 && notifications.height > 50);
        panel.visibility = false;
        verify(!panel.enabled);
        wait(350);
        compare(panel.width, 0);
    }
    function test_quick_list_wheel_speed_tail_clamp_and_reduce_motion() {
        const actionsBefore = JSON.stringify(DeviceActions.lastRequest);
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
        compare(JSON.stringify(DeviceActions.lastRequest), actionsBefore);
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
        compare(DeviceActions.lastRequest.join("|"), "wifi-disconnect|wlan0");
        findChild(popup, "availableWifiAction").clicked();
        compare(DeviceActions.lastRequest.join("|"), "wifi-connect|Guest");
        findChild(popup, "quickWifiPower").clicked();
        compare(DeviceActions.lastRequest.join("|"), "wifi-radio|off");
        verify(!findChild(popup, "quickSettingsLink"));
        NacrePanelState.openDeviceSettings("network");
        compare(NacrePanelState.screens.test.controlSection, "network");
    }

    function test_bluetooth_shows_known_devices_and_links_settings() {
        const popup = createTemporaryObject(bluetooth, test);
        compare(popup.known.length, 1);
        compare(popup.known[0].name, "Headphones");
        findChild(popup, "quickBluetoothPower").clicked();
        compare(DeviceActions.lastRequest.join("|"), "bluetooth-power|off");
        verify(!findChild(popup, "quickSettingsLink"));
        NacrePanelState.openDeviceSettings("bluetooth");
        compare(NacrePanelState.screens.test.controlSection, "bluetooth");
    }

    function test_display_never_changes_devices_or_history_and_rejects_stale_rows() {
        const wifi = createTemporaryObject(network, test);
        const bt = createTemporaryObject(bluetooth, test);
        const audio = createTemporaryObject(sound, test);
        const previous = DeviceActions.lastRequest.join("|");
        verify(!wifi.activate({
            bssid: "AA:BB:CC:DD:EE:99",
            ssid: "Missing",
            active: false
        }));
        verify(!bt.activate({
            address: "AA:BB:CC:DD:EE:99",
            connected: false
        }));
        compare(DeviceActions.lastRequest.join("|"), previous);
        DeviceActions.busy = true;
        verify(!wifi.activate(wifi.nearby[0]));
        verify(!bt.activate(bt.known[0]));
        compare(DeviceActions.lastRequest.join("|"), previous);
        DeviceActions.busy = false;
        verify(bt.activate(bt.known[0]));
        compare(DeviceActions.lastRequest.join("|"), "bluetooth-disconnect|AA:BB:CC:DD:EE:FF");
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
            compare(popup.currentItem, null);
        }
        popup.currentName = "toString";
        popup.hasCurrent = true;
        verify(!popup.hasCurrent);
        compare(popup.currentItem, null);
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
