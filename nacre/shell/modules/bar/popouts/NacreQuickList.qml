import QtQuick
import QtQuick.Controls.Basic
import qs.widgets

Flickable {
    id: root
    property real maximumHeight: 210
    property bool embedded: false
    implicitWidth: 320
    implicitHeight: Math.max(0, embedded ? contentHeight : Math.min(maximumHeight, contentHeight))
    contentWidth: width
    contentHeight: contentItem.children.length ? contentItem.children[0].implicitHeight : 0
    clip: true
    interactive: !embedded && contentHeight > height
    boundsBehavior: Flickable.StopAtBounds
    flickableDirection: Flickable.VerticalFlick
    maximumFlickVelocity: 2200
    flickDeceleration: 2800
    ScrollBar.vertical: NacreScrollBar {
        visible: !root.embedded
    }
    FastScroll {
        objectName: "quickListScroll"
        view: root
        enabled: !root.embedded
        step: 240
    }
}
