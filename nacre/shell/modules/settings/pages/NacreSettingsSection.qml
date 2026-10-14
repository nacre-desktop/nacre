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
    implicitHeight: body.visible ? body.y + body.implicitHeight + NacreTokens.space.xl : header.y + header.implicitHeight + NacreTokens.space.xl
    implicitWidth: NacreTokens.space.huge * 27 + NacreTokens.space.lg
    width: parent?.width ?? implicitWidth
    radius: NacreTokens.radii.medium
    color: NacreColours.palette.m3surfaceContainer
    Column {
        id: header
        x: NacreTokens.space.xl
        y: NacreTokens.space.xl
        width: parent.width - NacreTokens.space.xl * 2 - (root.collapsible ? NacreTokens.space.huge + NacreTokens.space.xs : 0)
        spacing: NacreTokens.space.sm
        NacreText {
            width: parent.width
            text: root.title
            visible: text.length > 0
            color: NacreTokens.ink
            font.pointSize: NacreTokens.textSize.title
            wrapMode: Text.Wrap
        }
        NacreText {
            width: parent.width
            text: root.description
            visible: text.length > 0
            color: NacreTokens.mutedInk
            font.pointSize: NacreTokens.textSize.caption
            wrapMode: Text.Wrap
        }
    }
    NacreSurface {
        objectName: "settingsSectionToggle"
        visible: root.collapsible
        x: parent.width - NacreTokens.space.huge - NacreTokens.space.lg
        y: NacreTokens.space.lg
        width: NacreTokens.space.huge
        height: NacreTokens.space.huge
        radius: NacreTokens.space.lg
        color: "transparent"
        NacreIcon {
            anchors.centerIn: parent
            text: root.expanded ? "expand_less" : "expand_more"
        }
        NacreInteraction {
            accessibleName: root.expanded ? NacreI18n.tr("settings.section.collapse", "Collapse {title}", {
                title: root.title
            }) : NacreI18n.tr("settings.section.expand", "Expand {title}", {
                title: root.title
            })
            function onClicked() {
                root.expanded = !root.expanded;
            }
        }
    }
    Column {
        id: body
        x: NacreTokens.space.xl
        y: header.y + header.height + NacreTokens.space.lg
        width: parent.width - NacreTokens.space.xl * 2
        spacing: NacreTokens.space.lg
        visible: !root.collapsible || root.expanded
    }
}
