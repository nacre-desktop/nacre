pragma Singleton
import QtQuick
import qs.services

QtObject {
    readonly property var workspaceNames: NacreProfile.data.workspaceNames || []
    readonly property int workspaceCount: NacreProfile.data.workspaceCount || 7
    readonly property var workspaceIcons: NacreProfile.data.workspaceIcons || []
    property QtObject sizes: QtObject {
        property int innerHeight: 30
        property int batteryWidth: 200
    }
}
