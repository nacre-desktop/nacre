import QtQuick

Window {
    id: root
    property bool grabFocus: false
    property QtObject anchor: QtObject {
        property var item: null
        property var window: null
        property rect rect: Qt.rect(0, 0, 1, 1)
        property int edges: 0
        property int gravity: 0
        function updateAnchor() {}
    }
}
