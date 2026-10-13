import QtQuick
import QtQuick.Window

Window {
    signal windowConnected
    signal closed
    property real implicitWidth: 0
    property real implicitHeight: 0
    width: implicitWidth
    height: implicitHeight
    property int exclusiveZone: 0
    property int exclusionMode: 0
    property Anchors anchors: Anchors {}
    property QtObject mask
    property bool above: true
    property bool minimized: false
    property size minimumSize: Qt.size(0, 0)
    property size maximumSize: Qt.size(0, 0)
}
