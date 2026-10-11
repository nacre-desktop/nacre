import QtQuick
import qs.services

Item {
    id: root
    required property var screen
    property Item statusItem: null
    property Item calendarItem: null
    readonly property var panel: NacrePanelState.panels[screen.name]
    property alias handler: observer
    anchors.fill: parent
    visible: panel?.input?.modal === true
    z: 100
    function inside(item, point) {
        if (!item)
            return false;
        const origin = item.mapToItem(root, 0, 0);
        return point.x >= origin.x && point.y >= origin.y && point.x < origin.x + item.width && point.y < origin.y + item.height;
    }
    function press(event) {
        const position = Qt.point(event.x, event.y);
        if (root.panel?.popouts?.pinned && (root.inside(root.statusItem, position) || root.inside(root.calendarItem, position))) {
            event.accepted = false;
            return;
        }
        if (root.panel?.input?.modal)
            root.panel.input.outsideClick(position);
    }
    MouseArea {
        id: observer
        anchors.fill: parent
        acceptedButtons: Qt.AllButtons
        onPressed: event => root.press(event)
        onDoubleClicked: event => root.press(event)
    }
}
