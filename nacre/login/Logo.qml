import QtQuick
import "translation.js" as Translation
import "messages.js" as Messages
import "LogoData.js" as LogoData

Item {
    id: root
    property string language: "system"
    function tr(id, fallback, args = {}) {
        return Translation.translate(Messages.english, language, id, fallback, args);
    }
    implicitWidth: 30
    implicitHeight: 30
    readonly property bool ready: image.loadedOnce
    property bool compact: width <= 36
    property bool ai: false
    property bool brain: false
    property bool settings: false
    property bool colorsApp: false
    property bool notifications: false
    property color primary: "#dda1ba"
    property color secondary: "#afa2df"
    property color tertiary: "#8cc9cd"
    property color highlight: "#eee9f4"
    property color background: "#151310"
    property color foreground: "#f4f1ef"
    property bool motionEnabled: false
    Accessible.name: root.ai ? root.tr("ui.brand-logo.name.nacre-ai", "Nacre AI") : root.brain ? root.tr("ui.brand-logo.name.nacre-brain", "Nacre Brain") : root.settings ? root.tr("ui.brand-logo.name.nacre-settings", "Nacre Settings") : root.colorsApp ? root.tr("ui.brand-logo.name.nacre-colors", "Nacre Colors") : root.notifications ? root.tr("ui.brand-logo.name.nacre-notifications", "Nacre Notifications") : root.tr("ui.brand-logo.name.nacre", "Nacre")
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
