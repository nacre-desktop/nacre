// Dependency fake: the LED reader itself is staged unchanged from production.
import QtQuick
import Quickshell

QtObject {
    property string path: ""
    property bool watchChanges: false
    property bool printErrors: false
    property bool blockLoading: false
    signal loaded
    signal fileChanged
    function text() {
        return Quickshell.files[path] || "";
    }
    function reload() {
        Qt.callLater(() => loaded());
    }
    onPathChanged: reload()
}
