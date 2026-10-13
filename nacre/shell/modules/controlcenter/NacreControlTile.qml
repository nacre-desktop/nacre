import QtQuick
import qs.widgets

NacreSurface {
    id: root
    property string label: ""
    property string detail: ""
    property string icon: ""
    property bool selected: false
    signal clicked
    implicitHeight: Math.max(60, detailText.y + detailText.implicitHeight + 10)
    implicitWidth: 120
    radius: 14
    topLeftRadius: 7
    bottomRightRadius: 7
    color: selected ? Qt.alpha(NacreTokens.accent, .13) : Qt.alpha(NacreTokens.raised, .6)
    border.width: 1
    border.color: Qt.alpha(NacreTokens.outline, selected ? .6 : .25)
    opacity: enabled ? 1 : .45
    NacreIcon {
        x: 10
        y: 8
        text: root.icon
        font.pointSize: 13
        color: root.selected ? NacreTokens.accent : NacreTokens.mutedInk
    }
    NacreText {
        id: labelText
        x: 34
        y: 8
        width: parent.width - 44
        text: root.label
        font.pointSize: 10
        wrapMode: Text.Wrap
    }
    NacreText {
        id: detailText
        x: 10
        y: Math.max(33, labelText.y + labelText.implicitHeight + 6)
        width: parent.width - 20
        text: root.detail
        font.pointSize: 9
        color: NacreTokens.mutedInk
        wrapMode: Text.Wrap
    }
    NacreInteraction {
        accessibleName: root.label + " " + root.detail
        function onClicked() {
            root.clicked();
        }
    }
}
