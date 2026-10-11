import QtQuick
import qs.widgets

Item {
    id: root
    required property var entry
    property string colorHex: entry.hex
    signal picked(string value)
    signal inspect(var entry)
    signal leave
    width: 72
    height: 76
    activeFocusOnTab: true
    Keys.onReturnPressed: picked(colorHex)
    Keys.onSpacePressed: picked(colorHex)
    Accessible.onPressAction: picked(colorHex)
    Rectangle {
        x: 0
        y: 0
        width: parent.width
        height: 34
        radius: 8
        color: "#" + root.colorHex
        border.width: mouse.containsMouse ? 2 : 0
        border.color: NacreTokens.ink
    }
    NacreText {
        y: 41
        width: parent.width
        text: Math.round(root.entry.coverage * 1000) / 10 + "%"
        font.pointSize: 9
        color: NacreTokens.mutedInk
        horizontalAlignment: Text.AlignHCenter
    }
    Rectangle {
        id: ghost
        width: 30
        height: 30
        radius: 15
        color: "#" + root.colorHex
        visible: mouse.drag.active
        property string colorHex: root.colorHex
        Drag.active: mouse.drag.active
        Drag.keys: ["orient-color"]
        Drag.source: ghost
        Drag.hotSpot: Qt.point(15, 15)
        z: 100
    }
    MouseArea {
        id: mouse
        anchors.fill: parent
        hoverEnabled: true
        drag.target: ghost
        onEntered: root.inspect(root.entry)
        onExited: root.leave()
        onClicked: root.picked(root.colorHex)
        onReleased: {
            ghost.Drag.drop();
            ghost.x = 0;
            ghost.y = 0;
        }
    }
    Accessible.role: Accessible.Button
    Accessible.name: "Sample #" + colorHex + ", " + Math.round(entry.coverage * 1000) / 10 + " percent coverage; drag to an accent role"
}
