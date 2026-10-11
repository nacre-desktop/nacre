pragma Singleton
import QtQuick
import qs.services
import qs.widgets

QtObject {
    readonly property color colour: NacreTokens.body
    readonly property int thickness: DesktopSettings.data.frameWidth ?? 10
    readonly property int headerHeight: DesktopSettings.data.topEdge === false ? 0 : 40 + thickness
    readonly property int left: DesktopSettings.data.leftEdge === false ? 0 : thickness
    readonly property int right: DesktopSettings.data.rightEdge === false ? 0 : thickness
    readonly property int bottom: DesktopSettings.data.bottomEdge === false ? 0 : thickness
    readonly property int rounding: DesktopSettings.data.frameRounding ?? 25
}
