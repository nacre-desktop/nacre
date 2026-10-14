pragma ComponentBehavior: Bound
import QtQuick
import qs.widgets

Item {
    id: root
    required property var registry
    required property var capabilities
    signal navigate(string route)
    implicitHeight: NacreTokens.space.huge * 7
    NacreSurface {
        id: screen
        x: Math.max(NacreTokens.space.xxl * 2, parent.width * .13)
        y: NacreTokens.space.huge + NacreTokens.space.xl
        width: parent.width - 2 * x
        height: NacreTokens.space.huge * 4
        radius: NacreTokens.radii.medium
        color: NacreTokens.body
        border.width: 1
        border.color: Qt.alpha(NacreTokens.outline, .5)
        Row {
            anchors.horizontalCenter: parent.horizontalCenter
            y: parent.height * .6
            spacing: NacreTokens.space.sm
            Repeater {
                model: [0, 1, 2]
                NacreSurface {
                    required property int modelData
                    width: Math.max(24, screen.width * .17)
                    height: NacreTokens.space.huge
                    radius: NacreTokens.radii.small
                    color: Qt.alpha(NacreTokens.outline, .1)
                }
            }
        }
        NacreSurface {
            x: NacreTokens.space.md
            y: NacreTokens.space.sm
            width: parent.width - 2 * x
            height: NacreTokens.space.compact
            radius: NacreTokens.radii.full
            color: Qt.alpha(NacreTokens.accent, .35)
        }
    }
    ActionButton {
        objectName: "edge-top-bar"
        anchors.horizontalCenter: screen.horizontalCenter
        y: NacreTokens.space.hairline
        text: root.registry.text("settings.registry.panels.top-bar.label")
        icon: "toolbar"
        compact: true
        border.width: hover0.hovered ? 1 : 0
        border.color: NacreTokens.accent
        HoverHandler {
            id: hover0
        }
        onClicked: root.navigate("panels/top-bar")
    }
    ActionButton {
        objectName: "edge-dashboard"
        anchors.horizontalCenter: screen.horizontalCenter
        y: screen.y + NacreTokens.space.xxl
        text: root.registry.text("settings.registry.panels.dashboard.label")
        icon: "space_dashboard"
        compact: true
        border.width: hover1.hovered ? 1 : 0
        border.color: NacreTokens.accent
        HoverHandler {
            id: hover1
        }
        onClicked: root.navigate("panels/dashboard")
    }
    ActionButton {
        objectName: "edge-ai"
        visible: true
        x: 0
        y: screen.y + (screen.height - height) / 2
        width: Math.min(NacreTokens.space.huge * 4, implicitWidth)
        text: root.registry.text(root.capabilities.ai === true ? "settings.registry.panels.ai-sidebar.label" : "settings.registry.ui.empty-left-edge")
        icon: "neurology"
        compact: true
        border.width: hover2.hovered ? 1 : 0
        border.color: NacreTokens.accent
        HoverHandler {
            id: hover2
        }
        onClicked: root.navigate(root.capabilities.ai === true ? "panels/ai-sidebar" : "addons")
    }
    ActionButton {
        objectName: "edge-control-center"
        x: root.width - width
        y: screen.y + (screen.height - height) / 2
        width: Math.min(NacreTokens.space.huge * 4 + NacreTokens.space.md, implicitWidth)
        text: root.registry.text("settings.registry.panels.control-center.label")
        icon: "tune"
        compact: true
        border.width: hover3.hovered ? 1 : 0
        border.color: NacreTokens.accent
        HoverHandler {
            id: hover3
        }
        onClicked: root.navigate("panels/control-center")
    }
    ActionButton {
        objectName: "edge-bottom"
        visible: true
        anchors.horizontalCenter: screen.horizontalCenter
        y: screen.y + screen.height + NacreTokens.space.md
        text: root.registry.text(root.capabilities.bottomPanel === true ? "settings.registry.panels.bottom.label" : "settings.registry.ui.empty-edge")
        icon: "dock_to_bottom"
        compact: true
        border.width: hover4.hovered ? 1 : 0
        border.color: NacreTokens.accent
        HoverHandler {
            id: hover4
        }
        onClicked: root.navigate("panels/bottom")
    }
}
