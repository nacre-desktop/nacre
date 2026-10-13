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
        // An on-demand Wayland panel may learn the held modifier only after
        // its first press activates keyboard focus. Retain that passive press.
        acceptedModifiers: Qt.AnyModifier
        dragThreshold: centroid.modifiers & Qt.MetaModifier ? 16 : 32767
        acceptedButtons: Qt.LeftButton
        cursorShape: Qt.ClosedHandCursor
        grabPermissions: PointerHandler.CanTakeOverFromItems
        target: null
        onActiveChanged: {
            if (active && (centroid.modifiers & Qt.MetaModifier)) {
                const press = root.globalPoint(centroid.scenePressPosition);
                const current = root.globalPoint(centroid.scenePosition);
                NacrePearls.begin(root.kind, root.host, root.screen.name, press);
                if (NacrePearls.drag?.host === root.host)
                    NacrePearls.update(current);
            }
        }
        onActiveTranslationChanged: {
            const drag = NacrePearls.drag;
            if (active && drag?.host === root.host)
                NacrePearls.update(root.globalPoint(centroid.scenePosition));
        }
        // Losing a grab (e.g. focus/surface changes) is not a mouse release.
        onGrabChanged: (transition, point) => {
            if (transition === PointerDevice.UngrabExclusive && NacrePearls.drag?.host === root.host) {
                if (point.state === EventPoint.Released) {
                    NacrePearls.update(root.globalPoint(point.scenePosition));
                    NacrePearls.finish();
                } else if (!NacrePearls.drag.released)
                    NacrePearls.cancel();
            }
        }
        onCanceled: if (NacrePearls.drag?.host === root.host && !NacrePearls.drag.released)
            NacrePearls.cancel()
    }
}
