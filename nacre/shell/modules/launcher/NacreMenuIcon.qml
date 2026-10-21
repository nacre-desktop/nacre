import QtQuick
import Quickshell
import qs.widgets
import qs.services

Item {
    id: root
    required property var entry
    property real size: 38
    implicitWidth: size
    implicitHeight: size
    clip: true
    Image {
        id: image
        anchors.fill: parent
        source: (root.entry.kind === "app" || root.entry.tui === true) && !root.entry.brand ? Quickshell.iconPath(root.entry.icon || "application-x-executable", true) : ""
        sourceSize.width: root.size
        sourceSize.height: root.size
        asynchronous: true
        visible: source !== "" && status === Image.Ready
        fillMode: Image.PreserveAspectFit
    }
    // Unbranded rows must not construct/decode an invisible SVG logo.
    Loader {
        anchors.fill: parent
        active: !!root.entry.brand
        sourceComponent: BrandLogo {
            objectName: "nacreAiAppLogo"
            ai: root.entry.brand === "ai"
            brain: root.entry.brand === "brain"
            settings: root.entry.brand === "settings"
            colorsApp: root.entry.brand === "colors"
        }
    }
    NacreText {
        anchors.centerIn: parent
        visible: !!root.entry.emoji
        text: root.entry.emoji || ""
        font.family: "Noto Color Emoji"
        font.pointSize: root.size * .6
    }
    NacreIcon {
        anchors.centerIn: parent
        visible: !root.entry.brand && !root.entry.emoji && !image.visible
        textFormat: Text.PlainText
        // Desktop icon names are image identifiers, not symbol-font ligatures.
        text: (root.entry.kind === "app" || root.entry.tui === true) ? "apps" : root.entry.icon || "chevron_right"
        font.pointSize: root.size * .5
        color: NacreColours.palette.m3onSurface
    }
}
