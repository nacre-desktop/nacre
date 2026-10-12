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
    height: 56
    MouseArea {
        anchors.fill: parent
        onPressed: root.window.startSystemMove()
        onDoubleClicked: root.window.maximized = !root.window.maximized
    }
    NacreTearHandle {
        x: 12
        y: 12
        kind: root.entry.kind
        host: root.window.bodyItem
        screenName: root.window.screen?.name || root.entry.origin
        pearl: root.entry
    }
    NacreText {
        x: 52
        anchors.verticalCenter: parent.verticalCenter
        width: Math.max(0, parent.width - 202)
        text: ({
                ai: "Nacre AI",
                launcher: "Launcher",
                dashboard: "Dashboard",
                controls: "Controls",
                media: "Media"
            })[root.entry.kind]
        font.pointSize: 13
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
            icon: "close"
            compact: true
            accessibleLabel: "Close window"
            onClicked: root.closeRequested()
        }
    }
}
