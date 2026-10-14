import QtQuick
import QtQuick.Controls.Basic
import qs.widgets
import qs.services

Flickable {
    id: root
    property int currentIndex: 0
    property var order: NacreDashboardState.tabs
    readonly property var names: NacreDashboardRegistry.names
    readonly property int selectedSlot: Math.max(0, order.indexOf(names[currentIndex]))
    readonly property real slotWidth: Math.max(58, width / Math.max(1, order.length))
    signal selected(int index)
    implicitHeight: 44
    contentWidth: Math.max(width, order.length * slotWidth)
    contentHeight: height
    clip: true
    boundsBehavior: Flickable.StopAtBounds
    flickableDirection: Flickable.HorizontalFlick
    function step(delta) {
        const index = Math.max(0, Math.min(order.length - 1, selectedSlot + delta));
        selected(names.indexOf(order[index]));
    }
    function revealSelection() {
        const left = selectedSlot * slotWidth, right = left + slotWidth;
        if (left < contentX)
            contentX = left;
        else if (right > contentX + width)
            contentX = Math.max(0, right - width);
    }
    onSelectedSlotChanged: Qt.callLater(revealSelection)
    onWidthChanged: Qt.callLater(revealSelection)
    onOrderChanged: Qt.callLater(revealSelection)
    onContentWidthChanged: Qt.callLater(revealSelection)
    Component.onCompleted: Qt.callLater(revealSelection)
    Rectangle {
        id: pearl
        objectName: "dashboardSelection"
        x: root.selectedSlot * root.slotWidth + (root.slotWidth - width) / 2
        y: 31
        width: 8
        height: 8
        radius: 4
        border.width: .5
        border.color: Qt.alpha(NacreTokens.ink, .18)
        gradient: Gradient {
            GradientStop {
                position: 0
                color: NacreTokens.orientHighlight ?? NacreTokens.accent
            }
            GradientStop {
                position: .45
                color: NacreTokens.accent
            }
            GradientStop {
                position: 1
                color: NacreTokens.accent
            }
        }
        Rectangle {
            x: -2
            y: -2
            width: 12
            height: 12
            radius: 6
            color: Qt.alpha(NacreTokens.accent, .12)
            z: -1
        }
        Rectangle {
            x: 1.5
            y: 1
            width: 3
            height: 2
            radius: 1
            color: Qt.alpha(NacreTokens.light ? NacreTokens.body : NacreTokens.ink, .85)
        }
    }
    Row {
        Repeater {
            model: root.order
            delegate: Item {
                required property string modelData
                required property int index
                readonly property int pageIndex: root.names.indexOf(modelData)
                objectName: "dashboardTab" + pageIndex
                width: root.slotWidth
                height: root.height
                Row {
                    anchors.horizontalCenter: parent.horizontalCenter
                    y: 6
                    spacing: 7
                    NacreIcon {
                        text: NacreDashboardRegistry.icon(parent.parent.modelData)
                        font.pointSize: 14
                        color: root.currentIndex === parent.parent.pageIndex ? NacreTokens.accent : NacreTokens.mutedInk
                    }
                    NacreText {
                        visible: root.slotWidth >= 94
                        text: NacreDashboardRegistry.title(parent.parent.modelData)
                        font.pointSize: 10
                        color: root.currentIndex === parent.parent.pageIndex ? NacreTokens.ink : NacreTokens.mutedInk
                    }
                }
                NacreInteraction {
                    id: interaction
                    accessibleName: NacreDashboardRegistry.title(parent.modelData)
                    function onClicked() {
                        root.selected(parent.pageIndex);
                    }
                }
                ToolTip {
                    id: hint
                    parent: interaction
                    visible: root.slotWidth < 94 && (interaction.hovered || interaction.keyboardFocusVisible)
                    delay: 450
                    text: interaction.accessibleName
                    contentItem: NacreText {
                        text: hint.text
                        font.pointSize: 9
                        color: NacreTokens.ink
                    }
                    background: NacreSurface {
                        radius: 10
                        color: NacreTokens.raised
                    }
                }
            }
        }
    }
    MouseArea {
        anchors.fill: parent
        acceptedButtons: Qt.NoButton
        onWheel: event => {
            const delta = event.angleDelta.y || event.angleDelta.x;
            if (delta && !wheelGate.running) {
                root.step(delta < 0 ? 1 : -1);
                wheelGate.restart();
            }
            event.accepted = true;
        }
    }
    Timer {
        id: wheelGate
        interval: 160
    }
}
