import QtQuick
import QtQuick.Controls.Basic
import qs.widgets

Flickable {
    id: root
    property real maximumHeight: 210
    implicitWidth: 320
    implicitHeight: Math.max(0, Math.min(maximumHeight, contentHeight))
    contentWidth: width
    contentHeight: contentItem.children.length ? contentItem.children[0].implicitHeight : 0
    clip: true
    boundsBehavior: Flickable.StopAtBounds
    flickableDirection: Flickable.VerticalFlick
    maximumFlickVelocity: 2200
    flickDeceleration: 2800
    ScrollBar.vertical: NacreScrollBar {}
    FastScroll {
        objectName: "quickListScroll"
        view: root
        step: 240
    }
}
