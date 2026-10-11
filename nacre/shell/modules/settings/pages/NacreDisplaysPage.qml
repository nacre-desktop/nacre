import QtQuick
import qs.widgets
import qs.services

NacreSettingsPage {
    id: root
    property string page: "displays"
    property string primary: DesktopSettings.monitors[0]?.name || ""
    readonly property bool blocked: DesktopSettings.pending || DesktopSettings.busy === true
    function connected(name) {
        return DesktopSettings.monitors.some(m => m.name === name);
    }
    function layout(mode) {
        if (!blocked && DesktopSettings.monitors.length > 1 && connected(primary))
            DesktopSettings.request(["display", mode, primary]);
    }
    function edit(name, scale, mode) {
        if (!blocked && connected(name))
            DesktopSettings.request(["display-edit", name, String(scale), mode]);
    }
    NacreSettingsSection {
        title: "Displays"
        description: "Choose a primary monitor, then extend or mirror connected screens."
        Flow {
            width: parent.width
            spacing: 8
            Repeater {
                model: DesktopSettings.monitors
                ActionButton {
                    required property var modelData
                    text: modelData.name
                    selected: root.primary === modelData.name
                    onClicked: root.primary = modelData.name
                }
            }
        }
        Flow {
            width: parent.width
            spacing: 8
            ActionButton {
                text: "Extend right"
                enabled: !root.blocked && DesktopSettings.monitors.length > 1
                onClicked: root.layout("extend-right")
            }
            ActionButton {
                text: "Extend left"
                enabled: !root.blocked && DesktopSettings.monitors.length > 1
                onClicked: root.layout("extend-left")
            }
            ActionButton {
                text: "Mirror"
                enabled: !root.blocked && DesktopSettings.monitors.length > 1
                onClicked: root.layout("mirror")
            }
        }
    }
    Repeater {
        model: DesktopSettings.monitors
        NacreSettingsSection {
            id: monitor
            required property var modelData
            title: modelData.name + " · " + modelData.width + "×" + modelData.height + " · " + Math.round(modelData.refreshRate || 0) + " Hz"
            description: "Display changes revert after 20 seconds unless you keep them."
            Flow {
                width: parent.width
                spacing: 8
                Repeater {
                    model: [1, 1.25, 1.5, 1.75, 2, 2.5, 3]
                    ActionButton {
                        required property real modelData
                        text: Math.round(modelData * 100) + "%"
                        selected: monitor.modelData.scale === modelData
                        enabled: !root.blocked
                        onClicked: root.edit(monitor.modelData.name, modelData, "")
                    }
                }
            }
            Flow {
                width: parent.width
                spacing: 8
                Repeater {
                    model: monitor.modelData.availableModes || []
                    ActionButton {
                        required property string modelData
                        text: modelData
                        enabled: !root.blocked
                        onClicked: root.edit(monitor.modelData.name, monitor.modelData.scale, modelData)
                    }
                }
            }
        }
    }
    NacreSettingsSection {
        title: "Keep this display change?"
        visible: DesktopSettings.pending
        Flow {
            width: parent.width
            spacing: 8
            ActionButton {
                text: "Keep"
                onClicked: DesktopSettings.request(["confirm"])
            }
            ActionButton {
                text: "Revert"
                onClicked: DesktopSettings.request(["revert"])
            }
        }
    }
    NacreText {
        width: parent.width
        text: DesktopSettings.message || ""
        font.pointSize: 10
        color: NacreTokens.mutedInk
        wrapMode: Text.Wrap
    }
}
