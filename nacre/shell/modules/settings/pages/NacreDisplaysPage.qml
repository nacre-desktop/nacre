import QtQuick
import qs.widgets
import qs.services

NacreSettingsPage {
    id: root
    property string page: "displays"
    property string primary: DesktopSettings.monitors[0]?.name || ""
    readonly property bool blocked: NacreCompositor.capabilities?.monitorConfiguration === false || DesktopSettings.pending || DesktopSettings.busy === true
    function connected(name) {
        return DesktopSettings.monitors.some(m => m.name === name);
    }
    function layout(mode) {
        if (!blocked && (mode !== "mirror" || NacreCompositor.capabilities?.monitorMirroring !== false) && DesktopSettings.monitors.length > 1 && connected(primary))
            DesktopSettings.request(["display", mode, primary]);
    }
    function edit(name, scale, mode) {
        if (!blocked && connected(name))
            DesktopSettings.request(["display-edit", name, String(scale), mode]);
    }
    NacreSettingsSection {
        title: NacreI18n.tr("ui.displays-page.title.displays", "Displays")
        description: NacreCompositor.capabilities?.monitorConfiguration === false ? NacreI18n.tr("ui.compositor.niri-display-limits", "Display transactions are unavailable on this compositor. Configure outputs in its output settings.") : NacreCompositor.capabilities?.monitorMirroring === false ? NacreI18n.tr("ui.compositor.niri-mirroring-limit", "Choose a primary monitor, then extend connected screens. Output mirroring is unavailable on niri.") : NacreI18n.tr("ui.displays-page.description.choose-a-primary-monitor-then-extend-or-mirror-connecte", "Choose a primary monitor, then extend or mirror connected screens.")
        Flow {
            width: parent.width
            spacing: NacreTokens.space.sm
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
            spacing: NacreTokens.space.sm
            ActionButton {
                text: NacreI18n.tr("ui.displays-page.text.extend-right", "Extend right")
                enabled: !root.blocked && DesktopSettings.monitors.length > 1
                onClicked: root.layout("extend-right")
            }
            ActionButton {
                text: NacreI18n.tr("ui.displays-page.text.extend-left", "Extend left")
                enabled: !root.blocked && DesktopSettings.monitors.length > 1
                onClicked: root.layout("extend-left")
            }
            ActionButton {
                text: NacreI18n.tr("ui.displays-page.text.mirror", "Mirror")
                enabled: !root.blocked && NacreCompositor.capabilities?.monitorMirroring !== false && DesktopSettings.monitors.length > 1
                onClicked: root.layout("mirror")
            }
        }
    }
    Repeater {
        model: DesktopSettings.monitors
        NacreSettingsSection {
            id: monitor
            required property var modelData
            title: NacreI18n.tr("ui.displays-page.title.value0-value1-value2-value3-hz", "{value0} · {value1}×{value2} · {value3} Hz", {
                value0: modelData.name,
                value1: modelData.width,
                value2: modelData.height,
                value3: Math.round(modelData.refreshRate || 0)
            })
            description: NacreI18n.tr("ui.displays-page.description.display-changes-revert-after-20-seconds-unless-you-keep", "Display changes revert after 20 seconds unless you keep them.")
            Flow {
                width: parent.width
                spacing: NacreTokens.space.sm
                Repeater {
                    model: [1, 1.25, 1.5, 1.75, 2, 2.5, 3]
                    ActionButton {
                        required property real modelData
                        text: NacreI18n.number(Math.round(modelData * 100)) + "%"
                        selected: monitor.modelData.scale === modelData
                        enabled: !root.blocked
                        onClicked: root.edit(monitor.modelData.name, modelData, "")
                    }
                }
            }
            Flow {
                width: parent.width
                spacing: NacreTokens.space.sm
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
        title: NacreI18n.tr("ui.displays-page.title.keep-this-display-change", "Keep this display change?")
        visible: DesktopSettings.pending
        Flow {
            width: parent.width
            spacing: NacreTokens.space.sm
            ActionButton {
                text: NacreI18n.tr("ui.displays-page.text.keep", "Keep")
                onClicked: DesktopSettings.request(["confirm"])
            }
            ActionButton {
                text: NacreI18n.tr("ui.displays-page.text.revert", "Revert")
                onClicked: DesktopSettings.request(["revert"])
            }
        }
    }
    NacreText {
        width: parent.width
        text: DesktopSettings.message || ""
        font.pointSize: NacreTokens.textSize.caption
        color: NacreTokens.mutedInk
        wrapMode: Text.Wrap
    }
}
