import QtQuick
import QtQuick.Window
import qs.services

// Super reserves this gesture; ordinary clicks, text selection and sliders remain
// owned by their controls. The original live panel moves in its layer surface.
Item {
    id: root
    required property string kind
    required property var host
    required property var screen
    function globalPoint(point) {
        return Qt.point(screen.x + point.x, screen.y + point.y);
    }
    DragHandler {
        id: gesture
        enabled: NacrePearls.enabled && DesktopSettings.data.dragPanels !== false && root.host.visible && root.host.enabled
        onEnabledChanged: if (!enabled && NacrePearls.drag?.host === root.host && !NacrePearls.drag.released)
            NacrePearls.cancel()
        acceptedModifiers: Qt.MetaModifier
        acceptedButtons: Qt.LeftButton
        cursorShape: Qt.ClosedHandCursor
        grabPermissions: PointerHandler.CanTakeOverFromItems
        target: null
        onActiveChanged: {
            if (active) {
                const point = root.mapToItem(root.Window.window.contentItem, centroid.pressPosition.x, centroid.pressPosition.y);
                NacrePearls.begin(root.kind, root.host, root.screen.name, root.globalPoint(point));
                if (NacrePearls.drag)
                    NacrePearls.update(Qt.point(NacrePearls.drag.start.x + activeTranslation.x, NacrePearls.drag.start.y + activeTranslation.y));
            }
        }
        onActiveTranslationChanged: {
            const drag = NacrePearls.drag;
            if (active && drag?.host === root.host)
                NacrePearls.update(Qt.point(drag.start.x + activeTranslation.x, drag.start.y + activeTranslation.y));
        }
        // Losing a grab (e.g. focus/surface changes) is not a mouse release.
        onGrabChanged: (transition, point) => {
            if (transition === PointerDevice.UngrabExclusive && NacrePearls.drag?.host === root.host) {
                if (point.state === EventPoint.Released)
                    NacrePearls.finish();
                else if (!NacrePearls.drag.released)
                    NacrePearls.cancel();
            }
        }
        onCanceled: if (NacrePearls.drag?.host === root.host && !NacrePearls.drag.released)
            NacrePearls.cancel()
    }
}
