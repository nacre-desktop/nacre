import QtQuick
import qs.widgets

NacreSurface {
    id: root
    property string label: ""
    property string detail: ""
    property string icon: ""
    property bool selected: false
    signal clicked
    implicitHeight: 60
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
        x: 34
        y: 8
        width: parent.width - 44
        text: root.label
        font.pointSize: 10
        elide: Text.ElideRight
    }
    NacreText {
        x: 10
        y: 33
        width: parent.width - 20
        text: root.detail
        font.pointSize: 9
        color: NacreTokens.mutedInk
        elide: Text.ElideRight
    }
    NacreInteraction {
        accessibleName: root.label + " " + root.detail
        function onClicked() {
            root.clicked();
        }
    }
}
