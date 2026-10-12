import QtQuick

QtObject {
    id: root
    required property var record
    required property var owner
    readonly property string windowId: record.windowId
    readonly property string address: windowId
    readonly property string appId: record.appId || ""
    readonly property string wmClass: appId
    readonly property string initialClass: appId
    readonly property string title: record.title || ""
    readonly property string initialTitle: title
    readonly property int pid: record.pid || 0
    readonly property var workspaceId: record.workspaceId
    readonly property var workspace: owner.workspaces.values.find(value => value.id === workspaceId) || null
    readonly property string outputId: record.outputId || ""
    readonly property var geometry: record.geometry
    readonly property bool geometryKnown: record.geometryKnown === true
    readonly property real x: geometry?.x ?? 0
    readonly property real y: geometry?.y ?? 0
    readonly property real width: geometry?.width ?? 0
    readonly property real height: geometry?.height ?? 0
    readonly property bool floating: record.floating === true
    readonly property bool urgent: record.urgent === true
    readonly property bool hidden: false
    readonly property bool pinned: false
    readonly property bool fullscreen: false
    readonly property int fullscreenMode: 0
    readonly property bool fullscreenKnown: false
    readonly property int focusHistoryId: -1
    readonly property var captureHandle: null
    readonly property var lastIpcObject: record
}
