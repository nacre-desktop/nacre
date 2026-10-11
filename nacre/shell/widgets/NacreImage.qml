import QtQuick
import QtQuick.Window

Image {
    id: root
    autoTransform: true
    property string path: ""
    property bool loadOriginal: false
    property bool active: true
    property int releaseDelay: 5000
    property size requestedSize: Qt.size(0, 0)
    property bool ready: false
    property bool retained: true
    property url displayedSource: ""
    property size decodeSize: Qt.size(-1, -1)
    property string error: ""
    asynchronous: true
    cache: true
    retainWhileLoading: true
    fillMode: Image.PreserveAspectCrop
    source: displayedSource
    function urlFor(value) {
        return /^(file|https?|image|qrc|data):/.test(value) ? value : value ? "file://" + encodeURIComponent(value).replace(/%2F/g, "/") : "";
    }
    function refresh() {
        if (!ready || !retained)
            return;
        error = "";
        decodeSize = loadOriginal ? Qt.size(-1, -1) : requestedSize.width > 0 && requestedSize.height > 0 ? requestedSize : Qt.size(Math.max(1, Math.ceil(width * Screen.devicePixelRatio)), Math.max(1, Math.ceil(height * Screen.devicePixelRatio)));
        sourceSize = decodeSize;
        displayedSource = urlFor(path);
    }
    function schedule(delay = 50) {
        if (ready && retained) {
            pending.interval = delay;
            pending.restart();
        }
    }
    onPathChanged: schedule(0)
    onWidthChanged: schedule()
    onHeightChanged: schedule()
    onLoadOriginalChanged: schedule()
    onRequestedSizeChanged: schedule()
    onActiveChanged: {
        if (!ready)
            return;
        if (active) {
            release.stop();
            retained = true;
            schedule(0);
        } else
            release.restart();
    }
    Screen.onDevicePixelRatioChanged: schedule()
    onStatusChanged: {
        if (status === Image.Error) {
            error = "Image could not be loaded.";
            displayedSource = "";
        } else if (status === Image.Ready)
            error = "";
    }
    Component.onCompleted: {
        ready = true;
        retained = active;
        refresh();
    }
    Timer {
        id: release
        interval: Math.max(0, root.releaseDelay)
        onTriggered: if (!root.active) {
            pending.stop();
            root.retained = false;
            root.displayedSource = "";
        }
    }
    Timer {
        id: pending
        interval: 50
        onTriggered: root.refresh()
    }
}
