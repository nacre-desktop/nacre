import QtQuick

QtObject {
    property var command: []
    property var environment: ({})
    property bool running: false
    property bool stdinEnabled: false
    property QtObject stdout
    property QtObject stderr
    property var signals: []
    function signal(value) {
        signals = [...signals, value];
    }
    property var writes: []
    property int starts: 0
    signal started
    signal exited(int exitCode, int exitStatus)
    function write(text) {
        writes = [...writes, text];
    }
    function finish(code = 0, status = 0) {
        running = false;
        exited(code, status);
    }
    onRunningChanged: if (running) {
        starts++;
        Qt.callLater(() => {
            if (running)
                started();
        });
    }
}
