import QtQuick
import qs.services

Item {
    id: root
    property string title: "Nacre"
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
        x: 20
        width: 32
        height: 32
        anchors.verticalCenter: parent.verticalCenter
        colorsApp: root.app === "colors"
        settings: root.app === "settings"
    }
    NacreText {
        x: 64
        anchors.verticalCenter: parent.verticalCenter
        text: root.title
        font.pointSize: 15
    }
    Row {
        anchors.right: parent.right
        anchors.rightMargin: 20
        anchors.verticalCenter: parent.verticalCenter
        spacing: 18
        NacreText {
            visible: !!root.section
            text: root.section
            color: NacreTokens.mutedInk
            anchors.verticalCenter: parent.verticalCenter
            font.pointSize: 11
        }
        NacreTabStrip {
            tabs: root.tabs
            current: root.currentTab
            onChosen: name => root.navigate(name)
        }
        Row {
            spacing: 4
            visible: !!root.mode
            Repeater {
                model: ["dark", "light"]
                ActionButton {
                    required property string modelData
                    objectName: modelData + "ModeButton"
                    text: ""
                    accessibleLabel: modelData === "dark" ? "Dark preview" : "Light preview"
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
