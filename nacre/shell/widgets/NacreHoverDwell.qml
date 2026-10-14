import QtQuick

Item {
    id: root
    property int duration: 350
    property real progress: 0
    property bool waiting: false
    property bool animatedCycle: false
    property int remaining: duration
    signal triggered
    function cancel() {
        waiting = false;
        fill.stop();
        delay.stop();
        progress = 0;
    }
    function start() {
        cancel();
        waiting = true;
        animatedCycle = NacreTokens.motionEnabled;
        if (animatedCycle)
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
        triggered();
    }
    // Opening follows the rendered fill's completion, never a competing timer.
    NumberAnimation {
        id: fill
        objectName: "hoverDwellAnimation"
        target: root
        property: "progress"
        from: 0
        to: 1
        duration: root.duration
        easing.type: Easing.Linear
        onFinished: root.finish()
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
