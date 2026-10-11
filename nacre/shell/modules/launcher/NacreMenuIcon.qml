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
    Image {
        id: image
        anchors.fill: parent
        source: root.entry.kind === "app" && !root.entry.brand ? Quickshell.iconPath(root.entry.icon || "application-x-executable", true) : ""
        sourceSize.width: root.size
        sourceSize.height: root.size
        asynchronous: true
        visible: source !== "" && status === Image.Ready
        fillMode: Image.PreserveAspectFit
    }
    BrandLogo {
        objectName: "nacreAiAppLogo"
        anchors.fill: parent
        visible: !!root.entry.brand
        ai: root.entry.brand === "ai"
        brain: root.entry.brand === "brain"
        settings: root.entry.brand === "settings"
        colorsApp: root.entry.brand === "colors"
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
        text: root.entry.icon || "chevron_right"
        font.pointSize: root.size * .5
        color: NacreColours.palette.m3onSurface
    }
}
