pragma ComponentBehavior: Bound
import QtQuick
import qs.widgets

Item {
    id: root
    required property var registry
    required property var capabilities
    signal navigate(string route)
    implicitHeight: 300
    NacreSurface {
        id: screen
        x: Math.max(48, parent.width * .13)
        y: 50
        width: parent.width - 2 * x
        height: 190
        radius: NacreTokens.radii.medium
        color: NacreTokens.body
        border.width: 1
        border.color: Qt.alpha(NacreTokens.outline, .5)
        Row {
            anchors.centerIn: parent
            spacing: NacreTokens.space.sm
            Repeater {
                model: [0, 1, 2]
                NacreSurface {
                    required property int modelData
                    width: Math.max(24, screen.width * .17)
                    height: modelData === 1 ? 66 : 44
                    radius: NacreTokens.radii.small
                    color: Qt.alpha(NacreTokens.outline, .1)
                }
            }
        }
        NacreSurface {
            x: NacreTokens.space.md
            y: NacreTokens.space.sm
            width: parent.width - 2 * x
            height: 10
            radius: NacreTokens.radii.full
            color: Qt.alpha(NacreTokens.accent, .35)
        }
    }
    ActionButton {
        objectName: "edge-top-bar"
        anchors.horizontalCenter: screen.horizontalCenter
        y: 2
        text: root.registry.text("settings.registry.panels.top-bar.label")
        icon: "toolbar"
        compact: true
        onClicked: root.navigate("panels/top-bar")
    }
    ActionButton {
        objectName: "edge-dashboard"
        anchors.horizontalCenter: screen.horizontalCenter
        y: screen.y + NacreTokens.space.xxl
        text: root.registry.text("settings.registry.panels.dashboard.label")
        icon: "space_dashboard"
        compact: true
        onClicked: root.navigate("panels/dashboard")
    }
    ActionButton {
        objectName: "edge-ai"
        visible: root.capabilities.ai === true
        x: 0
        y: screen.y + (screen.height - height) / 2
        width: Math.min(120, implicitWidth)
        text: root.registry.text("settings.registry.panels.ai-sidebar.label")
        icon: "neurology"
        compact: true
        onClicked: root.navigate("panels/ai-sidebar")
    }
    ActionButton {
        objectName: "edge-control-center"
        x: root.width - width
        y: screen.y + (screen.height - height) / 2
        width: Math.min(138, implicitWidth)
        text: root.registry.text("settings.registry.panels.control-center.label")
        icon: "tune"
        compact: true
        onClicked: root.navigate("panels/control-center")
    }
    ActionButton {
        objectName: "edge-bottom"
        visible: root.capabilities.bottomPanel === true
        anchors.horizontalCenter: screen.horizontalCenter
        y: screen.y + screen.height + NacreTokens.space.md
        text: root.registry.text("settings.registry.panels.bottom.label")
        icon: "dock_to_bottom"
        compact: true
        onClicked: root.navigate("panels/bottom")
    }
}
