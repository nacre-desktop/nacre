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
            visible: DesktopSettings.monitors.length > 1
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
            visible: DesktopSettings.monitors.length > 1
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
                visible: NacreCompositor.capabilities?.monitorMirroring !== false
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
            Row {
                width: parent.width
                spacing: NacreTokens.space.md
                NacreText {
                    width: Math.max(0, parent.width - scale.width - parent.spacing)
                    anchors.verticalCenter: parent.verticalCenter
                    text: NacreI18n.tr("settings.displays.scale", "Scale")
                }
                NacreSelect {
                    id: scale
                    readonly property var scales: [1, 1.25, 1.5, 1.75, 2, 2.5, 3]
                    width: Math.min(parent.width / 2, NacreTokens.space.huge * 8)
                    model: scales.map(value => Math.round(value * 100) + "%")
                    currentIndex: scales.indexOf(monitor.modelData.scale)
                    enabled: !root.blocked
                    onActivated: index => root.edit(monitor.modelData.name, scales[index], "")
                }
            }
            Row {
                width: parent.width
                spacing: NacreTokens.space.md
                visible: (monitor.modelData.availableModes || []).length > 0
                NacreText {
                    width: Math.max(0, parent.width - mode.width - parent.spacing)
                    anchors.verticalCenter: parent.verticalCenter
                    text: NacreI18n.tr("settings.displays.mode", "Resolution and refresh rate")
                    wrapMode: Text.Wrap
                }
                NacreSelect {
                    id: mode
                    width: Math.min(parent.width / 2, NacreTokens.space.huge * 10)
                    model: monitor.modelData.availableModes || []
                    searchable: model.length > 8
                    currentIndex: model.findIndex(value => value.startsWith(monitor.modelData.width + "x" + monitor.modelData.height + "@") && Math.abs(parseFloat(value.split("@")[1]) - monitor.modelData.refreshRate) < .01)
                    enabled: !root.blocked
                    onActivated: index => root.edit(monitor.modelData.name, monitor.modelData.scale, model[index])
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
