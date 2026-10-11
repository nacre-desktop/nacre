pragma Singleton
import QtQuick
import Quickshell

Singleton {
    id: root
    property string settingsPage: NacrePanelState.settingsPage
    property bool hidden: NacrePanelState.hidden
    readonly property real reveal: NacrePanelState.reveal
    property var screens: NacrePanelState.screens
    property var panels: NacrePanelState.panels
    onSettingsPageChanged: if (settingsPage !== NacrePanelState.settingsPage)
        NacrePanelState.settingsPage = settingsPage
    onHiddenChanged: if (hidden !== NacrePanelState.hidden)
        NacrePanelState.hidden = hidden
    onScreensChanged: if (screens !== NacrePanelState.screens)
        NacrePanelState.screens = screens
    onPanelsChanged: if (panels !== NacrePanelState.panels)
        NacrePanelState.panels = panels
    Connections {
        target: NacrePanelState
        function onSettingsPageChanged() {
            root.settingsPage = NacrePanelState.settingsPage;
        }
        function onHiddenChanged() {
            root.hidden = NacrePanelState.hidden;
        }
        function onScreensChanged() {
            root.screens = NacrePanelState.screens;
        }
        function onPanelsChanged() {
            root.panels = NacrePanelState.panels;
        }
    }
    function getForActive() {
        return NacrePanelState.getForActive();
    }
    function openEdge(name, screenName) {
        return NacrePanelState.openEdge(name, screenName);
    }
    function popout(name, center, screenName) {
        return NacrePanelState.popout(name, center, screenName);
    }
    function openMode(mode, query, preview) {
        return NacrePanelState.openMode(mode, query, preview);
    }
    function openDeviceSettings(icon) {
        return NacrePanelState.openDeviceSettings(icon);
    }
    function openSettings(page) {
        return NacrePanelState.openSettings(page);
    }
    function toggleLeft() {
        return NacrePanelState.toggleLeft();
    }
}
