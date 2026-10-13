import QtQuick

QtObject {
    property string path: ""
    property bool preload: false
    property bool printErrors: false
    property bool watchChanges: false
    property bool atomicWrites: true
    property string textValue: ""
    property int writes: 0
    signal loaded
    signal loadFailed(int error)
    signal fileChanged
    signal saved
    signal saveFailed(int error)
    function text() {
        return textValue;
    }
    function setText(value) {
        textValue = value;
        writes++;
        saved();
    }
    function reload() {
    }
}
