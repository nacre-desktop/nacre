import QtQuick
import qs.widgets
import qs.services

Item {
    id: root
    property int currentIndex: 0
    property var order: NacreDashboardState.tabs
    readonly property var names: ["today", "weather", "media", "system", "focus"]
    readonly property var labels: ["Dashboard", "Weather", "Media", "System", "Focus"]
    readonly property var icons: ["dashboard", "partly_cloudy_day", "queue_music", "speed", "timer"]
    readonly property int selectedSlot: Math.max(0, order.indexOf(names[currentIndex]))
    signal selected(int index)
    implicitHeight: 44
    function step(delta) {
        const index = Math.max(0, Math.min(order.length - 1, selectedSlot + delta));
        selected(names.indexOf(order[index]));
    }
    Rectangle {
        objectName: "dashboardSelection"
        x: root.selectedSlot * root.width / root.order.length + (root.width / root.order.length - width) / 2
        y: 34
        width: 7
        height: 7
        radius: 3.5
        color: "#f2efee"
        gradient: Gradient {
            GradientStop {
                position: 0
                color: "#fffdfb"
            }
            GradientStop {
                position: 1
                color: Qt.tint("#d0d1d6", Qt.alpha(NacreTokens.orientSecondary, .22))
            }
        }
        Behavior on x {
            NumberAnimation {
                duration: NacreTokens.motionEnabled ? 180 : 0
                easing.type: Easing.OutCubic
            }
        }
    }
    Row {
        anchors.fill: parent
        Repeater {
            model: root.order
            delegate: Item {
                required property string modelData
                required property int index
                readonly property int pageIndex: root.names.indexOf(modelData)
                objectName: "dashboardTab" + pageIndex
                width: root.width / root.order.length
                height: root.height
                Row {
                    anchors.horizontalCenter: parent.horizontalCenter
                    y: 6
                    spacing: 8
                    NacreIcon {
                        text: root.icons[parent.parent.pageIndex]
                        font.pointSize: 14
                        color: root.currentIndex === parent.parent.pageIndex ? NacreTokens.accent : NacreTokens.mutedInk
                    }
                    NacreText {
                        visible: root.width / root.order.length >= 94
                        text: root.labels[parent.parent.pageIndex]
                        font.pointSize: 11
                        color: root.currentIndex === parent.parent.pageIndex ? NacreTokens.ink : NacreTokens.mutedInk
                    }
                }
                NacreInteraction {
                    accessibleName: root.labels[parent.pageIndex]
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
