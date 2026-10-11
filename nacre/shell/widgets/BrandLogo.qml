import QtQuick
import qs.services
import "../branding/LogoData.js" as LogoData

Item {
    id: root
    implicitWidth: 30
    implicitHeight: 30
    readonly property bool ready: image.loadedOnce
    property bool compact: width <= 36
    property bool ai: false
    property bool brain: false
    property bool settings: false
    property bool colorsApp: false
    property color primary: NacreColours.palette.m3primary
    property color secondary: NacreColours.palette.m3secondary
    property color tertiary: NacreColours.palette.m3tertiary || secondary
    property color highlight: NacreColours.palette.m3primaryFixed || primary
    property color background: NacreColours.palette.m3frame || NacreColours.palette.m3surface
    property color foreground: NacreColours.palette.m3onSurface
    property bool motionEnabled: DesktopSettings.data.animations !== false
    Accessible.name: root.ai ? "Nacre AI" : root.brain ? "Nacre Brain" : root.settings ? "Nacre Settings" : root.colorsApp ? "Nacre Colors" : "Nacre"
    Accessible.role: Accessible.Graphic
    Image {
        id: image
        anchors.fill: parent
        fillMode: Image.PreserveAspectFit
        asynchronous: true
        retainWhileLoading: true
        sourceSize.width: Math.max(32, Math.ceil(root.width * 3))
        sourceSize.height: Math.max(32, Math.ceil(root.height * 3))
        source: "data:image/svg+xml;utf8," + encodeURIComponent(LogoData.colored(root.primary, root.secondary, root.tertiary, root.highlight, root.background, root.foreground, root.compact, root.ai, root.brain, root.settings, root.colorsApp))
        property bool loadedOnce: false
        onStatusChanged: if (status === Image.Ready) {
            if (loadedOnce && root.motionEnabled)
                reveal.restart();
            loadedOnce = true;
        }
        NumberAnimation {
            id: reveal
            target: image
            property: "opacity"
            from: .96
            to: 1
            duration: 160
        }
    }
}
