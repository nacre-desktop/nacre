import QtQuick
import qs.widgets
import qs.services

Flickable {
    id: root
    property int currentIndex: 0
    property var order: NacreDashboardState.tabs
    readonly property var names: NacreDashboardRegistry.names
    readonly property int selectedSlot: Math.max(0, order.indexOf(names[currentIndex]))
    readonly property real slotWidth: Math.max(58, Math.min(170, width / Math.min(6, order.length)))
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
    onSelectedSlotChanged: revealSelection()
    onWidthChanged: revealSelection()
    Rectangle {
        objectName: "dashboardSelection"
        x: root.selectedSlot * root.slotWidth + (root.slotWidth - width) / 2
        y: 36
        width: 7
        height: 7
        radius: 3.5
        color: NacreTokens.accent
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
                    accessibleName: NacreDashboardRegistry.title(parent.modelData)
                    function onClicked() {
                        root.selected(parent.pageIndex);
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
