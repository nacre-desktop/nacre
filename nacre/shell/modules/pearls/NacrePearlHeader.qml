import QtQuick
import QtQuick.Window
import qs.widgets
import qs.services

Item {
    id: root
    required property var entry
    required property var window
    signal dockRequested
    signal closeRequested
    signal pinRequested
    height: 40
    MouseArea {
        anchors.fill: parent
        onPressed: root.window.startSystemMove()
        onDoubleClicked: root.window.maximized = !root.window.maximized
    }
    NacreText {
        x: 14
        anchors.verticalCenter: parent.verticalCenter
        width: Math.max(0, parent.width - (root.entry.kind === "launcher" ? 310 : 180))
        text: ({
                ai: "Nacre AI",
                launcher: "Launcher",
                dashboard: "Dashboard",
                controls: "Controls",
                media: "Media"
            })[root.entry.kind]
        font.pointSize: 11
        elide: Text.ElideRight
    }
    Row {
        anchors.right: parent.right
        anchors.rightMargin: 12
        anchors.verticalCenter: parent.verticalCenter
        spacing: 4
        ActionButton {
            visible: root.entry.kind === "launcher"
            text: root.window.closeAfterLaunch ? "Auto close" : "Stay open"
            compact: true
            accessibleLabel: "Toggle closing this launcher after opening an app"
            onClicked: root.window.closeAfterLaunch = !root.window.closeAfterLaunch
        }
        ActionButton {
            text: ""
            icon: "dock_to_left"
            compact: true
            accessibleLabel: "Dock to edge"
            onClicked: root.dockRequested()
        }
        ActionButton {
            text: ""
            icon: "push_pin"
            compact: true
            accessibleLabel: "Pin to all workspaces"
            onClicked: root.pinRequested()
        }
        ActionButton {
            text: ""
            icon: "open_in_full"
            compact: true
            accessibleLabel: "Maximize or restore window"
            onClicked: root.window.maximized = !root.window.maximized
        }
        ActionButton {
            text: ""
            icon: "close"
            compact: true
            accessibleLabel: "Close window"
            onClicked: root.closeRequested()
        }
    }
}
