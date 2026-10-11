import QtQuick
import qs.widgets
import qs.services

NacreSurface {
    id: root
    property string title: ""
    property string description: ""
    property bool collapsible: false
    property bool expanded: true
    default property alias contents: body.data
    implicitHeight: 32 + header.implicitHeight + (body.visible ? 14 + body.implicitHeight : 0)
    implicitWidth: 880
    width: parent?.width ?? implicitWidth
    radius: 18
    color: NacreColours.palette.m3surfaceContainer
    Column {
        id: header
        x: 16
        y: 16
        width: parent.width - 32 - (root.collapsible ? 36 : 0)
        spacing: 8
        NacreText {
            width: parent.width
            text: root.title
            visible: text.length > 0
            color: NacreTokens.accent
            font.pointSize: 13
            wrapMode: Text.Wrap
        }
        NacreText {
            width: parent.width
            text: root.description
            visible: text.length > 0
            color: NacreTokens.mutedInk
            font.pointSize: 10
            wrapMode: Text.Wrap
        }
    }
    NacreSurface {
        objectName: "settingsSectionToggle"
        visible: root.collapsible
        x: parent.width - 46
        y: 14
        width: 30
        height: 30
        radius: 15
        color: "transparent"
        NacreIcon {
            anchors.centerIn: parent
            text: root.expanded ? "expand_less" : "expand_more"
        }
        NacreInteraction {
            accessibleName: (root.expanded ? "Collapse " : "Expand ") + root.title
            function onClicked() {
                root.expanded = !root.expanded;
            }
        }
    }
    Column {
        id: body
        x: 16
        y: header.y + header.height + 14
        width: parent.width - 32
        spacing: 14
        visible: !root.collapsible || root.expanded
    }
}
