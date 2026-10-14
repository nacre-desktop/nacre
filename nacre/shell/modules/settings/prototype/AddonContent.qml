import QtQuick
import qs.widgets

Column {
    id: root
    required property var registry
    required property var capabilities
    signal action(string action)
    spacing: NacreTokens.space.xl
    ActionButton {
        visible: !root.capabilities.ai
        text: root.registry.text("settings.registry.nacre-ai.label")
        icon: "neurology"
        onClicked: root.action("install-ai")
    }
    ActionButton {
        visible: !root.capabilities.brain
        text: root.registry.text("settings.registry.brain.label")
        icon: "auto_stories"
        onClicked: root.action("install-brain")
    }
}
