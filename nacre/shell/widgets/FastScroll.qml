import QtQuick

WheelHandler {
    id: root

    required property var view
    property real step: 600
    property real pixelMultiplier: 3.0
    property real destination: 0
    property int smoothDuration: 210
    property bool motionEnabled: NacreTokens.motionEnabled
    readonly property bool horizontal: view?.orientation === ListView.Horizontal || view?.flickableDirection === Flickable.HorizontalFlick
    property bool kinetic: true
    property real velocity: 0
    property real lastPixelTime: 0
    property int pixelSamples: 0
    readonly property Timer release: Timer {
        interval: 65
        onTriggered: {
            if (root.motionEnabled && root.pixelSamples > 1 && Math.abs(root.velocity) > 100)
                root.view.flick(root.horizontal ? -Math.max(-2600, Math.min(2600, root.velocity)) : 0, root.horizontal ? 0 : -Math.max(-2600, Math.min(2600, root.velocity)));
            else
                root.settled();
            root.velocity = 0;
            root.pixelSamples = 0;
        }
    }

    readonly property Connections movement: Connections {
        function onMovementEnded() {
            root.destination = root.position();
            root.settled();
        }

        function onVisibleChanged() {
            if (!root.view.visible)
                root.cancel();
        }
        function onDraggingChanged() {
            if (root.view.dragging)
                root.cancel();
        }
        function onContentHeightChanged() {
            root.clamp();
        }
        function onHeightChanged() {
            root.clamp();
        }
        function onWidthChanged() {
            root.clamp();
        }
        function onContentWidthChanged() {
            root.clamp();
        }
        target: root.view
    }

    readonly property NumberAnimation motion: NumberAnimation {
        target: root.view
        property: root.horizontal ? "contentX" : "contentY"
        duration: root.motionEnabled ? root.smoothDuration : 0
        easing.type: Easing.OutCubic
        onFinished: root.settled()
    }

    signal scrolled
    signal settled

    function position() {
        return view ? view[horizontal ? "contentX" : "contentY"] : 0;
    }
    function origin() {
        return view ? (horizontal ? view.originX : view.originY) || 0 : 0;
    }
    function maximum() {
        return view ? origin() + Math.max(0, horizontal ? view.contentWidth - view.width : view.contentHeight - view.height) : 0;
    }
    function setPosition(value) {
        if (view)
            view[horizontal ? "contentX" : "contentY"] = value;
    }
    function clamp() {
        if (!view)
            return;
        const minimum = origin();
        const upper = maximum();
        if (position() < minimum || position() > upper) {
            cancel();
            setPosition(Math.max(minimum, Math.min(upper, position())));
        }
        destination = Math.max(minimum, Math.min(upper, destination));
        if (motion.running && (motion.to < minimum || motion.to > upper)) {
            motion.stop();
            motion.from = position();
            motion.to = destination;
            motion.start();
        }
    }

    onMotionEnabledChanged: if (!motionEnabled) {
        const finalPosition = motion.running ? destination : position();
        cancel();
        if (view)
            setPosition(Math.max(origin(), Math.min(maximum(), finalPosition)));
    }

    onHorizontalChanged: {
        cancel();
        destination = position();
    }

    function scrollBy(delta, smooth) {
        if (!view || !Number.isFinite(delta))
            return;
        view.cancelFlick();
        const base = motion.running ? destination : position();
        const next = Math.max(origin(), Math.min(maximum(), base + delta));
        // Repeated wheel input at an edge must not restart an unfinished glide.
        if (motion.running && next === destination)
            return;
        destination = next;
        motion.stop();
        if (smooth && motionEnabled) {
            motion.from = position();
            motion.to = destination;
            motion.start();
        } else {
            setPosition(destination);
        }
    }

    function pixelScroll(delta) {
        if (!view || !Number.isFinite(delta))
            return;
        const now = Date.now();
        const elapsed = now - lastPixelTime;
        if (elapsed > 120 || velocity * delta < 0) {
            velocity = 0;
            pixelSamples = 0;
        }
        const sample = delta * 1000 / Math.max(8, Math.min(50, elapsed || 16));
        velocity = pixelSamples ? velocity * 0.55 + sample * 0.45 : sample;
        lastPixelTime = now;
        pixelSamples++;
        scrollBy(delta, false);
        if (kinetic && motionEnabled)
            release.restart();
        else
            settled();
    }

    function cancel() {
        release.stop();
        motion.stop();
        if (view)
            view.cancelFlick();
        velocity = 0;
        pixelSamples = 0;
    }

    target: null
    acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
    onWheel: event => {
        scrolled();
        const pixels = horizontal ? event.pixelDelta.x || event.pixelDelta.y : event.pixelDelta.y;
        const angles = horizontal ? event.angleDelta.x || event.angleDelta.y : event.angleDelta.y;
        if (pixels) {
            pixelScroll(-pixels * pixelMultiplier);
        } else if (angles) {
            release.stop();
            velocity = 0;
            pixelSamples = 0;
            scrollBy(-angles / 120 * step, true);
        }
        event.accepted = true;
    }
}
