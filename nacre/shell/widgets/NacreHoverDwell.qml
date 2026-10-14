import QtQuick

Item {
    id: root
    property int duration: 350
    property real progress: 0
    property bool waiting: false
    property bool animatedCycle: false
    property int remaining: duration
    property int fullFrames: 2
    signal triggered
    function cancel() {
        waiting = false;
        fill.stop();
        delay.stop();
        progress = 0;
        fullFrames = 2;
    }
    function start() {
        cancel();
        waiting = true;
        animatedCycle = NacreTokens.motionEnabled;
        if (duration <= 0) {
            progress = 1;
            finish();
        } else if (animatedCycle)
            fill.restart();
        else {
            progress = 1;
            remaining = duration;
            delay.restart();
        }
    }
    function finish() {
        if (!waiting)
            return;
        waiting = false;
        fill.stop();
        triggered();
    }
    function advance(seconds) {
        if (!waiting || !animatedCycle)
            return;
        if (progress < 1) {
            // A stalled frame must not consume the entire remaining visible fill.
            const step = Math.max(0, Math.min(.035, seconds));
            progress = Math.min(1, progress + step * 1000 / Math.max(1, duration));
        } else if (--fullFrames <= 0)
            finish();
    }
    // Keep the completed trace visible before lazy popup creation starts.
    FrameAnimation {
        id: fill
        objectName: "hoverDwellAnimation"
        onTriggered: root.advance(frameTime)
    }
    Timer {
        id: delay
        interval: root.remaining
        onTriggered: root.finish()
    }
    Connections {
        target: NacreTokens
        function onMotionEnabledChanged() {
            if (!NacreTokens.motionEnabled && root.waiting && root.animatedCycle) {
                root.remaining = Math.max(1, Math.round(root.duration * (1 - root.progress)));
                fill.stop();
                root.animatedCycle = false;
                root.progress = 1;
                delay.restart();
            }
        }
    }
}
