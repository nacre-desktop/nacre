import QtQuick
import QtQuick.Window

Window {
    id: root
    signal windowConnected
    signal closed
    property bool grabFocus: false
    property QtObject anchor: QtObject {
        property var item: null
        property var window: null
        property rect rect: Qt.rect(0, 0, 1, 1)
        property int edges: 0
        property int gravity: 0
        function updateAnchor() {}
    }
    onVisibleChanged: visible ? windowConnected() : closed()
}
