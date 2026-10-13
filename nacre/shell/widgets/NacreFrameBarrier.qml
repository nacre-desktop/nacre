import QtQuick

// Keep the source until two requested destination frames have been submitted.
// Request the second frame explicitly: a static panel need not animate again.
QtObject {
    id: root
    required property var surface
    required property var commit
    required property var reject
    property bool armed: false
    property int frames: 0
    function start() {
        frames = 0;
        armed = true;
        deadline.start();
        surface.update();
    }
    function settle(callback) {
        if (!armed)
            return;
        armed = false;
        deadline.stop();
        callback();
    }
    property Connections observer: Connections {
        target: root.surface
        enabled: root.armed
        function onFrameSwapped() {
            if (++root.frames >= 2)
                root.settle(root.commit);
            else
                root.surface.update();
        }
    }
    property Timer deadline: Timer {
        interval: 500
        onTriggered: root.settle(root.reject)
    }
}
