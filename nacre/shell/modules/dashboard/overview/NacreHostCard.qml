import QtQuick
import Quickshell
import Quickshell.Io
import qs.widgets
import "overview.js" as Overview

NacreOverviewCard {
    id: root
    property bool active: false
    property int updateCount: -1
    signal updateRequested
    signal systemRequested
    property string kernel: ""
    property string username: Quickshell.env("USER") || "User"
    property string operatingSystem: "Linux"
    property real uptimeSeconds: NaN
    readonly property bool sampling: refresh.running
    readonly property var rows: [
        {
            icon: "badge",
            text: username
        },
        {
            icon: "computer",
            text: operatingSystem
        },
        {
            icon: "desktop_windows",
            text: "Hyprland"
        },
        {
            icon: "timer",
            text: Overview.uptime(uptimeSeconds),
            action: "system"
        }
    ].concat(updateCount >= 0 ? [
        {
            icon: "system_update",
            text: updateCount + " updates",
            action: "updates"
        }
    ] : []).concat(kernel ? [
        {
            icon: "memory",
            text: kernel
        }
    ] : [])
    FileView {
        path: root.active ? "/etc/os-release" : ""
        printErrors: false
        onLoaded: root.operatingSystem = Overview.osName(text())
    }
    FileView {
        id: uptimeFile
        path: root.active ? "/proc/uptime" : ""
        printErrors: false
        onLoaded: root.uptimeSeconds = parseFloat(text())
    }
    Timer {
        id: refresh
        interval: 60000
        running: root.active && root.visible
        repeat: true
        onTriggered: uptimeFile.reload()
    }
    Item {
        id: logo
        x: 18
        anchors.verticalCenter: parent.verticalCenter
        width: root.width >= 320 ? 124 : 54
        height: width
        NacreSurface {
            anchors.fill: parent
            radius: width / 2
            color: NacreTokens.body
        }
        BrandLogo {
            anchors.centerIn: parent
            width: parent.width * .74
            height: parent.height * .74
        }
    }
    Column {
        x: logo.x + logo.width + 16
        anchors.verticalCenter: parent.verticalCenter
        width: Math.max(0, root.width - x - 14)
        spacing: root.rows.length > 5 ? 4 : 6
        Repeater {
            model: root.rows
            Item {
                required property var modelData
                required property int index
                width: parent.width
                height: root.rows.length > 5 ? 24 : 27
                NacreIcon {
                    text: parent.modelData.icon
                    color: parent.index % 2 ? NacreTokens.mutedInk : NacreTokens.accent
                    font.pointSize: 13
                }
                NacreText {
                    objectName: "hostLine" + parent.index
                    x: 25
                    width: Math.max(0, parent.width - 25)
                    text: parent.modelData.text
                    font.pointSize: root.width >= 320 ? 12 : 10
                    elide: Text.ElideRight
                }
                NacreInteraction {
                    objectName: parent.modelData.action ? "hostAction_" + parent.modelData.action : ""
                    visible: !!parent.modelData.action
                    disabled: !parent.modelData.action
                    accessibleName: parent.modelData.action === "system" ? "Open System tab" : "Open package updates"
                    function onClicked() {
                        if (parent.modelData.action === "system")
                            root.systemRequested();
                        else
                            root.updateRequested();
                    }
                }
            }
        }
    }
}
