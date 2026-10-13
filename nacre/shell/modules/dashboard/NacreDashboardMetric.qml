import QtQuick
import qs.widgets

NacreSurface {
    id: root
    property string label: ""
    property string value: ""
    property string detail: ""
    property string icon: ""
    property var history: []
    property real fraction: 0
    radius: 16
    color: NacreTokens.raised
    NacreIcon {
        x: 14
        y: 14
        text: root.icon
        color: NacreTokens.accent
        font.pointSize: 15
    }
    NacreText {
        x: 40
        y: 15
        width: parent.width - 52
        text: root.label
        font.pointSize: 10
        color: NacreTokens.mutedInk
        elide: Text.ElideRight
    }
    NacreText {
        x: 14
        y: root.height < 165 ? 33 : 44
        width: parent.width - 28
        text: root.value
        font.pointSize: root.height < 165 ? 17 : root.width < 155 ? 19 : 25
        font.weight: Font.DemiBold
        elide: Text.ElideRight
    }
    NacreText {
        x: 14
        y: root.height < 165 ? 61 : 84
        width: parent.width - 28
        text: root.detail
        font.pointSize: 9
        color: NacreTokens.mutedInk
        maximumLineCount: 2
        wrapMode: Text.Wrap
        elide: Text.ElideRight
    }
    NacreDashboardSparkline {
        x: 14
        y: parent.height - height - 12
        width: parent.width - 28
        height: root.height < 165 ? 16 : 34
        values: root.history
        visible: root.history.length > 0
    }
    Rectangle {
        x: 14
        y: parent.height - 19
        width: parent.width - 28
        height: 5
        radius: 2.5
        color: Qt.alpha(NacreTokens.accent, .12)
        visible: !root.history.length
        Rectangle {
            width: parent.width * Math.max(0, Math.min(1, root.fraction))
            height: parent.height
            radius: 2.5
            color: NacreTokens.accent
        }
    }
}
