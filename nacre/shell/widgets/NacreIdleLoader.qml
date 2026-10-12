import QtQuick

Loader {
    id: root
    property bool wanted: false
    property int releaseDelay: 5000
    property bool preserveState: false
    property var savedState: null
    property bool retained: false
    active: wanted || retained
    onWantedChanged: {
        if (wanted) {
            release.stop();
            retained = true;
        } else if (retained)
            release.restart();
    }
    onLoaded: if (preserveState && savedState && item?.restoreState)
        item.restoreState(savedState)
    Timer {
        id: release
        interval: Math.max(0, root.releaseDelay)
        onTriggered: if (!root.wanted) {
            if (root.preserveState && root.item?.captureState)
                root.savedState = root.item.captureState();
            root.retained = false;
        }
    }
}
