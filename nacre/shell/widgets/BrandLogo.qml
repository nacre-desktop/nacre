import QtQuick
import qs.services
import "../branding/LogoData.js" as LogoData

Item {
    id: root
    implicitWidth: NacreTokens.iconSize.hero
    implicitHeight: NacreTokens.iconSize.hero
    readonly property bool ready: image.loadedOnce
    property bool compact: width <= 36
    property bool ai: false
    property bool brain: false
    property bool settings: false
    property bool colorsApp: false
    property bool notifications: false
    property color primary: NacreColours.palette.m3primary
    property color secondary: NacreColours.palette.m3secondary
    property color tertiary: NacreColours.palette.m3tertiary || secondary
    property color highlight: NacreColours.palette.m3primaryFixed || primary
    property color background: NacreColours.palette.m3frame || NacreColours.palette.m3surface
    property color foreground: NacreColours.palette.m3onSurface
    property bool motionEnabled: DesktopSettings.data.animations !== false
    Accessible.name: root.ai ? NacreI18n.tr("ui.brand-logo.name.nacre-ai", "Nacre AI") : root.brain ? NacreI18n.tr("ui.brand-logo.name.nacre-brain", "Nacre Brain") : root.settings ? NacreI18n.tr("ui.brand-logo.name.nacre-settings", "Nacre Settings") : root.colorsApp ? NacreI18n.tr("ui.brand-logo.name.nacre-colors", "Nacre Colors") : root.notifications ? NacreI18n.tr("ui.brand-logo.name.nacre-notifications", "Nacre Notifications") : NacreI18n.tr("ui.brand-logo.name.nacre", "Nacre")
    Accessible.role: Accessible.Graphic
    Image {
        id: image
        anchors.fill: parent
        fillMode: Image.PreserveAspectFit
        asynchronous: true
        retainWhileLoading: true
        sourceSize.width: Math.max(32, Math.ceil(root.width * 3))
        sourceSize.height: Math.max(32, Math.ceil(root.height * 3))
        source: "data:image/svg+xml;utf8," + encodeURIComponent(LogoData.colored(root.primary, root.secondary, root.tertiary, root.highlight, root.background, root.foreground, root.compact, root.ai, root.brain, root.settings, root.colorsApp, root.notifications))
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
