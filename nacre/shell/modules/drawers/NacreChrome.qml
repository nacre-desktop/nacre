import QtQuick
import QtQuick.Shapes
import "contour.js" as Contour
import qs.config
import qs.widgets

Item {
    id: root
    required property var host
    property var lips: null
    readonly property real radius: Math.max(0, Math.min(NacreFrame.rounding, host.width / 2, host.height / 2))
    readonly property bool gallery: host.launcher.visible && host.launcher.fullScreenGallery === true
    readonly property var handles: lips?.rimHandles ?? []
    readonly property var silhouette: Contour.geometry(width, height, materialRects(), radius, handles)
    function materialRects() {
        const rects = [];
        if (NacreFrame.headerHeight > 0)
            rects.push(Qt.rect(0, 0, width, host.y));
        if (NacreFrame.left > 0)
            rects.push(Qt.rect(0, 0, host.x, height));
        if (NacreFrame.right > 0)
            rects.push(Qt.rect(host.x + host.width, 0, width - host.x - host.width, height));
        if (NacreFrame.bottom > 0)
            rects.push(Qt.rect(0, host.y + host.height, width, height - host.y - host.height));
        for (const panel of [host.leftDrawer, host.osd, host.session, host.dashboard, host.popouts, host.notifications, host.launcher]) {
            if (panel.width > 0 && panel.height > 0 && !(panel === host.launcher && gallery))
                rects.push(Qt.rect(host.x + panel.x, host.y + panel.y, panel.width, panel.height));
        }
        return rects.concat(handles);
    }
    Shape {
        anchors.fill: parent
        preferredRendererType: Shape.CurveRenderer
        ShapePath {
            fillColor: NacreTokens.body
            strokeWidth: 0
            fillRule: ShapePath.OddEvenFill
            PathSvg {
                path: root.silhouette.body
            }
        }
    }
}
