import QtQuick
import qs.services

Item {
    id: root
    property string title: NacreI18n.tr("ui.app-header.title.nacre", "Nacre")
    property string app: ""
    property var tabs: []
    property string currentTab: ""
    property string mode: ""
    property string section: ""
    signal navigate(string name)
    signal modeChangedByUser(string name)
    signal moveRequested
    height: 64
    MouseArea {
        anchors.fill: parent
        onPressed: root.moveRequested()
    }
    BrandLogo {
        x: NacreTokens.space.xl
        width: NacreTokens.iconSize.hero
        height: NacreTokens.iconSize.hero
        anchors.verticalCenter: parent.verticalCenter
        colorsApp: root.app === "colors"
        settings: root.app === "settings"
    }
    NacreText {
        x: 64
        anchors.verticalCenter: parent.verticalCenter
        text: root.title
        font.pointSize: NacreTokens.textSize.title
    }
    Row {
        anchors.right: parent.right
        anchors.rightMargin: NacreTokens.space.xl
        anchors.verticalCenter: parent.verticalCenter
        spacing: NacreTokens.space.xl
        NacreText {
            visible: !!root.section
            text: root.section
            color: NacreTokens.mutedInk
            anchors.verticalCenter: parent.verticalCenter
            font.pointSize: NacreTokens.textSize.caption
        }
        NacreTabStrip {
            tabs: root.tabs
            current: root.currentTab
            onChosen: name => root.navigate(name)
        }
        Row {
            spacing: NacreTokens.space.xs
            visible: !!root.mode
            Repeater {
                model: ["dark", "light"]
                ActionButton {
                    required property string modelData
                    objectName: modelData + "ModeButton"
                    text: ""
                    accessibleLabel: modelData === "dark" ? NacreI18n.tr("ui.app-header.accessibleLabel.dark-preview", "Dark preview") : NacreI18n.tr("ui.app-header.accessibleLabel.light-preview", "Light preview")
                    icon: modelData === "dark" ? "dark_mode" : "light_mode"
                    compact: true
                    selected: root.mode === modelData
                    onClicked: root.modeChangedByUser(modelData)
                }
            }
        }
    }
    Rectangle {
        anchors.bottom: parent.bottom
        width: parent.width
        height: 1
        color: NacreTokens.outline
        opacity: .35
    }
}
