import QtQuick
import QtQuick.Window
import QtQuick.Controls.Basic
import qs.widgets
import qs.services

Item {
    id: root
    property string kind: "dashboard"
    property var host: null
    property string screenName: ""
    property var pearl: null
    property point origin: Qt.point(0, 0)
    property point pending: Qt.point(0, 0)
    property bool querying: false
    width: 32
    height: 32
    visible: NacrePearls.enabled
    function globalPoint(x, y) {
        const point = mapToItem(root.Window.window.contentItem, x, y);
        const monitor = NacrePearls.screen(screenName);
        return pearl ? Qt.point(origin.x + point.x, origin.y + point.y) : Qt.point((monitor?.x || 0) + point.x, (monitor?.y || 0) + point.y);
    }
    Rectangle {
        anchors.centerIn: parent
        width: 16
        height: 16
        radius: 8
        gradient: Gradient {
            GradientStop {
                position: 0
                color: NacreTokens.orientHighlight
            }
            GradientStop {
                position: .55
                color: NacreTokens.orientSecondary
            }
            GradientStop {
                position: 1
                color: NacreTokens.accent
            }
        }
    }
    MouseArea {
        id: pointer
        anchors.fill: parent
        hoverEnabled: true
        cursorShape: Qt.OpenHandCursor
        onPressed: event => {
            root.pending = Qt.point(event.x, event.y);
            if (root.pearl) {
                root.querying = true;
                root.pearl.window.geometry(value => {
                    root.querying = false;
                    if (!pointer.pressed || value.error)
                        return;
                    root.origin = Qt.point(value.x, value.y);
                    NacrePearls.begin(root.kind, root.host, root.screenName, root.globalPoint(root.pending.x, root.pending.y), root.pearl);
                });
            } else
                NacrePearls.begin(root.kind, root.host, root.screenName, root.globalPoint(event.x, event.y));
        }
        onPositionChanged: event => {
            if (pressed && !root.querying)
                NacrePearls.update(root.globalPoint(event.x, event.y));
        }
        onReleased: NacrePearls.finish()
        onCanceled: NacrePearls.cancel()
    }
    ToolTip.visible: pointer.containsMouse
    ToolTip.delay: 600
    ToolTip.text: root.pearl ? "Drag pearl to dock or reposition" : "Pull to open in window"
    Accessible.role: Accessible.Button
    Accessible.name: root.pearl ? "Dock panel" : "Open panel in window"
    Accessible.onPressAction: root.pearl ? NacrePearls.dock(root.pearl) : NacrePearls.detach(root.kind, root.host, root.screenName)
}
