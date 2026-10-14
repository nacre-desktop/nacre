import QtQuick
import qs.services
import "prototype"

SettingsView {
    id: root
    property bool active: true
    registry: NacreSettingsRegistry
    store: NacreSettingsProvider
    capabilities: NacreSettingsCapabilities.snapshot
    onRouteOpened: route => {
        if (NacrePanelState.settingsPage !== route)
            NacrePanelState.settingsPage = route;
    }
    function captureState() {
        return {
            page: page,
            query: query
        };
    }
    function restoreState(data) {
        if (data.page === page)
            query = data.query || "";
    }
    function focusContent() {
        forceActiveFocus(Qt.OtherFocusReason);
    }
    Component.onCompleted: open(NacrePanelState.settingsPage || "panels")
    Connections {
        target: NacrePanelState
        function onSettingsPageChanged() {
            root.open(NacrePanelState.settingsPage);
        }
    }
}
