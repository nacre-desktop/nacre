import QtQuick
import qs.widgets

NacreSurface {
    id: root
    property string title: ""
    property string description: ""
    default property alias contents: body.data
    radius: NacreTokens.radii.medium
    color: NacreTokens.raised
    implicitHeight: body.y + body.implicitHeight + NacreTokens.space.xl
    Column {
        id: heading
        x: NacreTokens.space.xl
        y: NacreTokens.space.xl
        width: parent.width - 2 * x
        spacing: NacreTokens.space.xs
        NacreText {
            text: root.title
            width: parent.width
            wrapMode: Text.Wrap
            font.pointSize: NacreTokens.textSize.title
            font.weight: Font.Medium
        }
        NacreText {
            visible: text.length > 0
            text: root.description
            width: parent.width
            wrapMode: Text.Wrap
            color: NacreTokens.mutedInk
            font.pointSize: NacreTokens.textSize.caption
        }
    }
    Column {
        id: body
        x: NacreTokens.space.md
        y: heading.y + heading.height + NacreTokens.space.sm
        width: parent.width - 2 * x
        spacing: NacreTokens.space.xs
    }
}
