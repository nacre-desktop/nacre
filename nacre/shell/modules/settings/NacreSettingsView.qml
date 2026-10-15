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
            query: query,
            backStack: backStack
        };
    }
    function restoreState(data) {
        if (data.page === page)
            query = data.query || "";
        backStack = (current?.parent ? data.backStack || [] : []).filter(item => typeof item.route === "string" && Number.isFinite(item.position)).slice(-64);
    }
    function focusContent() {
        forceActiveFocus(Qt.OtherFocusReason);
    }
    Component.onCompleted: open(NacrePanelState.settingsPage || "panels", false)
    Connections {
        target: NacrePanelState
        function onSettingsPageChanged() {
            if (NacrePanelState.settingsPage !== root.page + (root.highlight ? "#" + root.highlight : ""))
                if (root.schema.pages.find(item => item.id === NacrePanelState.settingsPage.split("#")[0])?.parent)
                    root.open(NacrePanelState.settingsPage);
                else
                    root.navigate(NacrePanelState.settingsPage);
        }
    }
}
