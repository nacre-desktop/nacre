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
        property QtObject rect: QtObject {
            property real x: 0
            property real y: 0
            property real width: 1
            property real height: 1
        }
        property int edges: 0
        property int gravity: 0
        function updateAnchor() {}
    }
    onVisibleChanged: visible ? windowConnected() : closed()
}
