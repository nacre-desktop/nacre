import QtQuick
import QtQuick.Controls.Basic
import qs.widgets
import qs.services

NacreSurface {
    id: root
    required property string icon
    required property string label
    property color ink: NacreTokens.ink
    property bool selected: false
    signal clicked
    function focusControl() {
        input.forceActiveFocus(Qt.OtherFocusReason);
    }
    implicitWidth: 32
    implicitHeight: 32
    radius: 16
    color: selected ? NacreColours.palette.m3primaryContainer : "transparent"
    NacreIcon {
        anchors.centerIn: parent
        text: root.icon === "pause" ? "\ue034" : root.icon
        fill: root.icon === "pause" ? 1 : 0
        font.pointSize: 17
        color: root.selected ? NacreColours.palette.m3onPrimaryContainer : root.ink
    }
    NacreInteraction {
        id: input
        activeFocusOnTab: true
        accessibleName: root.label
        function onClicked() {
            root.clicked();
        }
    }
    ToolTip.visible: input.hovered
    ToolTip.delay: 650
    ToolTip.text: label
}
