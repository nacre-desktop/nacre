import QtQuick
import qs.widgets

Item {
    id: root
    required property var screen
    property string currentName: ""
    property real currentCenter: 0
    property bool hasCurrent: false
    property bool retaining: false
    property real lastWidth: 320
    property real lastHeight: 80
    readonly property var routes: ({
            audio: "NacreSoundPopup.qml",
            network: "NacreNetworkPopup.qml",
            bluetooth: "NacreBluetoothPopup.qml",
            notifications: "NacreHistoryPopup.qml",
            battery: "NacreBatteryPopup.qml",
            calendar: "NacreCalendarPopup.qml"
        })
    readonly property bool validRoute: Object.prototype.hasOwnProperty.call(routes, currentName)
    readonly property bool loaded: body.status === Loader.Ready && !!body.item
    readonly property string error: body.status === Loader.Error ? "This menu could not be loaded." : ""
    readonly property real targetWidth: lastWidth + 30
    readonly property real targetHeight: lastHeight + 30
    readonly property var currentItem: body.item
    implicitWidth: targetWidth
    implicitHeight: targetHeight
    function rememberSize() {
        if (!body.item)
            return;
        const w = body.item.implicitWidth, h = body.item.implicitHeight;
        if (Number.isFinite(w) && w > 0)
            lastWidth = Math.max(120, Math.min(640, w));
        if (Number.isFinite(h) && h > 0)
            lastHeight = Math.max(30, Math.min(640, h));
    }
    Loader {
        id: body
        objectName: "nacrePopupLoader"
        x: 15
        y: 15
        active: root.validRoute && (root.hasCurrent || root.retaining)
        source: root.validRoute ? Qt.resolvedUrl(root.routes[root.currentName]) : ""
        onLoaded: root.rememberSize()
        onStatusChanged: if (status === Loader.Error) {
            root.lastWidth = 280;
            root.lastHeight = 64;
        }
    }
    Connections {
        target: body.item
        function onImplicitWidthChanged() {
            root.rememberSize();
        }
        function onImplicitHeightChanged() {
            root.rememberSize();
        }
    }
    NacreText {
        x: 15
        y: 15
        width: root.lastWidth
        visible: root.error !== ""
        text: root.error
        wrapMode: Text.WordWrap
        maximumLineCount: 3
        color: NacreTokens.mutedInk
    }
}
