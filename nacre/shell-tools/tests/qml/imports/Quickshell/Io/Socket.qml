import QtQuick

QtObject {
    property string path: ""
    property bool connected: false
    property QtObject parser
    signal error(int error)
    function write(text) {
    }
}
