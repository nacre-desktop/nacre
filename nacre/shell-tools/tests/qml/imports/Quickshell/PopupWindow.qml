import QtQuick
import QtQuick.Window

Window {
    id: root
    signal windowConnected
    signal closed
    property bool grabFocus: false
    property alias anchor: anchorObject
    QtObject {
        id: anchorObject
        property var item: null
        property var window: null
        property int edges: 0
        function updateAnchor() {
        }
    }
    onVisibleChanged: visible ? windowConnected() : closed()
}
