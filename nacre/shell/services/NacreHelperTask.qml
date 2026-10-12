import QtQuick

QtObject {
    id: root
    property var command: []
    property bool running: false
    property bool stdinEnabled: false
    property bool issued: false
    property int requestId: 0
    signal started
    signal output(string text)
    signal exited(int code, int status)
    onRunningChanged: {
        if (running) {
            issued = false;
            Qt.callLater(begin);
        } else if (requestId) {
            NacreHelpers.cancel(requestId);
            requestId = 0;
        }
    }
    function begin() {
        if (!running || issued)
            return;
        started();
        if (running && !stdinEnabled && !issued)
            send(null);
    }
    function send(value) {
        if (!running || issued)
            return;
        issued = true;
        requestId = NacreHelpers.submit(root, value);
    }
    function write(text) {
        if (!running || issued)
            return;
        try {
            send(JSON.parse(text));
        } catch (error) {
            complete(1, 0, JSON.stringify({
                error: "Invalid helper input"
            }));
        }
    }
    function complete(code, status, text) {
        if (!running)
            return;
        requestId = 0;
        if (text)
            output(text);
        running = false;
        exited(code, status);
    }
    Component.onDestruction: if (requestId)
        NacreHelpers.cancel(requestId)
}
