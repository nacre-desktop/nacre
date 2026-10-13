import QtQuick

Item {
    id: root
    property var entry: null
    property bool running: false
    property bool active: false
    readonly property string mediaIdentity: JSON.stringify([entry?.path || "", entry?.poster || "", !!entry?.animated])
    visible: active
    function refresh() {
        active = false;
        settle.stop();
        if (running && entry?.dynamic)
            settle.start();
    }
    onRunningChanged: refresh()
    onMediaIdentityChanged: refresh()
    Component.onCompleted: refresh()
    Timer {
        id: settle
        interval: 180
        onTriggered: root.active = root.running && root.entry?.dynamic === true
    }
    Loader {
        anchors.fill: parent
        active: root.active && root.entry?.animated === true
        sourceComponent: animation
    }
    Component {
        id: animation
        AnimatedImage {
            source: "file://" + encodeURIComponent(root.entry.path).replace(/%2F/g, "/")
            asynchronous: true
            playing: root.active
            fillMode: Image.PreserveAspectCrop
        }
    }
    Loader {
        anchors.fill: parent
        active: root.active && !root.entry?.animated
        source: active ? Qt.resolvedUrl("NacreWallpaperVideo.qml") : ""
        onLoaded: item.path = Qt.binding(() => root.entry?.path || "")
    }
}
