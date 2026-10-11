import QtQuick
import QtQuick.Shapes
import QtQuick.Effects
import qs.services
import qs.widgets

Item {
    id: root
    required property var entry
    property bool selected: false
    property bool previewMotion: false
    width: 200
    height: 174
    signal chosen
    function inside(x, y) {
        if (x < 0 || y < 0 || x > width || y > height)
            return false;
        return Math.abs(x - width / 2) / (width / 2) + Math.abs(y - height / 2) / (height / 2) * 0.5 <= 1;
    }
    readonly property string outline: "M" + width / 4 + " 0 H" + width * 3 / 4 + " L" + width + " " + height / 2 + " L" + width * 3 / 4 + " " + height + " H" + width / 4 + " L0 " + height / 2 + " Z"
    Item {
        id: pixels
        anchors.fill: parent
        visible: false
        Image {
            anchors.fill: parent
            source: root.visible ? "file://" + (root.entry.thumbnail || root.entry.poster) : ""
            asynchronous: true
            cache: true
            retainWhileLoading: true
            fillMode: Image.PreserveAspectCrop
            sourceSize.width: 480
            sourceSize.height: 420
        }
        NacreWallpaperMotion {
            anchors.fill: parent
            entry: root.entry
            running: root.previewMotion
        }
    }
    Shape {
        id: mask
        anchors.fill: parent
        visible: false
        layer.enabled: true
        preferredRendererType: Shape.CurveRenderer
        ShapePath {
            fillColor: "white"
            strokeWidth: 0
            PathSvg {
                path: root.outline
            }
        }
    }
    MultiEffect {
        anchors.fill: parent
        source: pixels
        maskEnabled: true
        maskSource: mask
        maskThresholdMin: 0.5
        maskSpreadAtMin: 1
    }
    Shape {
        anchors.fill: parent
        preferredRendererType: Shape.CurveRenderer
        ShapePath {
            fillColor: "transparent"
            strokeColor: root.selected ? NacreColours.palette.m3primary : Qt.alpha(NacreColours.palette.m3outline, 0.35)
            strokeWidth: root.selected ? 2 : 1
            PathSvg {
                path: root.outline
            }
        }
    }
    NacreText {
        x: 24
        width: parent.width - 48
        y: parent.height - 44
        horizontalAlignment: Text.AlignHCenter
        elide: Text.ElideRight
        text: root.entry.name
        font.pointSize: 10
        color: "white"
        style: Text.Outline
        styleColor: "#aa000000"
    }
    QtObject {
        id: hit
        function contains(point: point): bool {
            return root.inside(point.x, point.y);
        }
    }
    MouseArea {
        anchors.fill: parent
        containmentMask: hit
        cursorShape: Qt.PointingHandCursor
        onClicked: root.chosen()
    }
}
