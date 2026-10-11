import QtQuick
import qs.widgets

Item {
    id: root
    property string path: ""
    property Image current: front
    property bool fading: false
    property Image pendingImage: null
    property Image previous: null
    readonly property bool hasImage: current.status === Image.Ready
    function request() {
        if (!path || fading)
            return;
        const image = hasImage ? (current === front ? back : front) : current;
        image.requestPath = path;
        image.source = "file://" + encodeURIComponent(path).replace(/%2F/g, "/");
        if (image.status === Image.Ready)
            ready(image);
    }
    function present(image) {
        if (image.status !== Image.Ready || image.requestPath !== path)
            return;
        if (!hasImage || image === current) {
            current = image;
            image.animateOpacity = false;
            image.opacity = 1;
            return;
        }
        previous = current;
        previous.animateOpacity = false;
        previous.opacity = 1;
        previous.z = 0;
        image.animateOpacity = false;
        image.opacity = 0;
        image.z = 1;
        current = image;
        image.animateOpacity = true;
        image.opacity = 1;
        fading = true;
        settle.restart();
    }
    function ready(image) {
        if (image.status === Image.Ready) {
            pendingImage = image;
            presentation.restart();
        }
    }
    Timer {
        id: presentation
        interval: 0
        onTriggered: root.present(root.pendingImage)
    }
    onPathChanged: request()
    Component.onCompleted: request()
    function finish() {
        settle.stop();
        if (previous) {
            previous.animateOpacity = false;
            previous.opacity = 0;
        }
        previous = null;
        current.animateOpacity = false;
        if (current.status === Image.Ready)
            current.opacity = 1;
        fading = false;
        if (current.requestPath !== path)
            request();
    }
    Connections {
        target: NacreTokens
        function onMotionEnabledChanged() {
            if (!NacreTokens.motionEnabled)
                root.finish();
        }
    }
    Timer {
        id: settle
        interval: NacreTokens.motionEnabled ? 320 : 0
        onTriggered: root.finish()
    }
    Image {
        id: front
        property string requestPath: ""
        property bool animateOpacity: false
        anchors.fill: parent
        asynchronous: true
        cache: true
        retainWhileLoading: true
        fillMode: Image.PreserveAspectCrop
        sourceSize.width: 1600
        sourceSize.height: 1000
        opacity: 0
        onStatusChanged: root.ready(this)
        Behavior on opacity {
            enabled: front.animateOpacity && NacreTokens.motionEnabled
            NumberAnimation {
                duration: NacreTokens.motionEnabled ? 300 : 0
                easing.type: Easing.InOutCubic
            }
        }
    }
    Image {
        id: back
        property string requestPath: ""
        property bool animateOpacity: false
        anchors.fill: parent
        asynchronous: true
        cache: true
        retainWhileLoading: true
        fillMode: Image.PreserveAspectCrop
        sourceSize.width: 1600
        sourceSize.height: 1000
        opacity: 0
        onStatusChanged: root.ready(this)
        Behavior on opacity {
            enabled: back.animateOpacity && NacreTokens.motionEnabled
            NumberAnimation {
                duration: NacreTokens.motionEnabled ? 300 : 0
                easing.type: Easing.InOutCubic
            }
        }
    }
}
