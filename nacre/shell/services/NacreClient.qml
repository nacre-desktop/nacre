import QtQuick

QtObject {
    id: root
    required property var nativeWindow
    required property var owner
    required property string address
    readonly property var lastIpcObject: nativeWindow?.lastIpcObject || ({})
    readonly property string wmClass: lastIpcObject.class || nativeWindow?.wayland?.appId || ""
    readonly property string title: nativeWindow?.title || lastIpcObject.title || ""
    readonly property string initialClass: lastIpcObject.initialClass || wmClass
    readonly property string initialTitle: lastIpcObject.initialTitle || title
    readonly property real x: Number(lastIpcObject.at?.[0] || 0)
    readonly property real y: Number(lastIpcObject.at?.[1] || 0)
    readonly property real width: Math.max(0, Number(lastIpcObject.size?.[0] || 0))
    readonly property real height: Math.max(0, Number(lastIpcObject.size?.[1] || 0))
    readonly property var workspace: nativeWindow?.workspace || owner.workspaces.values.find(workspace => workspace.id === lastIpcObject.workspace?.id) || null
    readonly property bool floating: lastIpcObject.floating === true
    readonly property bool fullscreen: Number(lastIpcObject.fullscreen || 0) !== 0
    readonly property int pid: Number(lastIpcObject.pid || 0)
    readonly property int focusHistoryId: Number(lastIpcObject.focusHistoryID ?? -1)
}
