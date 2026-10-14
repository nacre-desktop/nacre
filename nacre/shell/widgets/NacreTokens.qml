pragma Singleton

import QtQuick
import Quickshell.Io as Io
import qs.services

QtObject {
    id: root

    property string textFamily: DesktopSettings.data.interfaceFont || "IBM Plex Sans"
    property string monoFamily: "JetBrains Mono NF"
    readonly property Io.FileView sizeFile: Io.FileView {
        path: Qt.resolvedUrl("size-scale.json")
        blockLoading: true
    }
    readonly property var sizeScale: JSON.parse(sizeFile.text())
    readonly property var space: sizeScale.spacing
    readonly property var radii: sizeScale.radii
    readonly property var textSize: sizeScale.text
    readonly property var iconSize: sizeScale.icons
    property real textPointSize: textSize.body
    // Compatibility alias for modules awaiting redesign.
    property real cornerRadius: space.md
    property real spacing: space.sm
    property int colorDuration: 200
    property int stateDuration: 120
    property int textDuration: 400
    property bool reduceMotion: DesktopSettings.data.reduceMotion === true
    readonly property bool motionEnabled: !reduceMotion && DesktopSettings.data.animations !== false

    readonly property color body: NacreColours.palette.m3surface
    readonly property color raised: NacreColours.palette.m3surfaceContainer
    readonly property color frame: NacreColours.palette.m3frame
    readonly property color ink: NacreColours.palette.m3onSurface
    readonly property color mutedInk: NacreColours.palette.m3onSurfaceVariant
    readonly property color accent: NacreColours.palette.m3primary
    readonly property color orientSecondary: NacreColours.palette.m3secondary ?? accent
    readonly property color orientHighlight: Qt.tint(accent, Qt.alpha("#fff9f1", .22))
    readonly property color outline: NacreColours.palette.m3outline

    readonly property bool light: NacreColours.light
    property bool paletteSwitch: false
    readonly property bool colorMotionAllowed: motionEnabled && !paletteSwitch && NacreColours.publishing !== true
    onLightChanged: {
        // Light/dark foreground and surface changes must remain readable together.
        paletteSwitch = true;
        Qt.callLater(() => paletteSwitch = false);
    }

    function focusInk(background): color {
        function linear(channel) {
            return channel <= 0.04045 ? channel / 12.92 : Math.pow((channel + 0.055) / 1.055, 2.4);
        }
        const c = background ?? root.raised;
        const luminance = linear(c.r) * 0.2126 + linear(c.g) * 0.7152 + linear(c.b) * 0.0722;
        return luminance > 0.179 ? "#101014" : "#ffffff";
    }
}
