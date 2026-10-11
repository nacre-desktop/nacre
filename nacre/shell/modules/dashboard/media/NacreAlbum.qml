import QtQuick
import QtQuick.Shapes
import qs.widgets

Item {
    id: root
    property string source: ""
    property real progress: 0
    property var spectrum: []
    property color first: NacreTokens.accent
    property color second: NacreTokens.orientSecondary
    property bool ready: false
    implicitWidth: 224
    implicitHeight: 224
    NacreClip {
        id: mask
        anchors.fill: parent
        anchors.margins: 38
        radius: width / 2
        color: NacreTokens.raised
        Image {
            id: picture
            objectName: "fullAlbumImage"
            anchors.fill: parent
            source: root.source
            asynchronous: true
            cache: true
            retainWhileLoading: true
            fillMode: Image.PreserveAspectCrop
            sourceSize.width: 480
            sourceSize.height: 480
            visible: status === Image.Ready || status === Image.Loading && root.ready
            onStatusChanged: if (status === Image.Ready)
                root.ready = true
            else if (status === Image.Error || status === Image.Null)
                root.ready = false
        }
        NacreIcon {
            anchors.centerIn: parent
            text: "music_note"
            font.pointSize: 44
            visible: !picture.visible
            color: NacreTokens.mutedInk
        }
    }
    Canvas {
        id: spiral
        anchors.fill: parent
        property var values: root.spectrum
        property real trackProgress: root.progress
        property color first: root.first
        property color second: root.second
        onValuesChanged: requestPaint()
        onTrackProgressChanged: requestPaint()
        onFirstChanged: requestPaint()
        onSecondChanged: requestPaint()
        onWidthChanged: requestPaint()
        onPaint: {
            const ctx = getContext("2d");
            ctx.reset();
            const center = width / 2;
            for (let i = 0; i < 32; i++) {
                const t = i / 31, angle = -Math.PI / 2 + t * Math.PI * 2.6;
                const radius = width * (.29 + .14 * t);
                const amplitude = Math.max(0, Math.min(1, root.spectrum[i] || 0));
                const length = 3 + 5 * t + amplitude * 12;
                ctx.strokeStyle = Qt.alpha(Qt.tint(root.first, Qt.alpha(root.second, t)), root.spectrum.length || t < root.progress ? .85 : .25);
                ctx.lineWidth = 2 + t * 1.5;
                ctx.lineCap = "round";
                ctx.beginPath();
                ctx.moveTo(center + radius * Math.cos(angle), center + radius * Math.sin(angle));
                ctx.lineTo(center + (radius + length) * Math.cos(angle), center + (radius + length) * Math.sin(angle));
                ctx.stroke();
            }
        }
    }
}
