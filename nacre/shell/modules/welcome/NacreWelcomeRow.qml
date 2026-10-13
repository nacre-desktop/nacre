import QtQuick
import qs.widgets
import qs.services

NacreSurface {
    id: root
    required property string heading
    required property string detail
    property string icon: ""
    property real leadingSpace: root.icon ? 56 : 18
    property string buttonText: NacreI18n.tr("welcome.welcome-row.buttonText.open", "Open")
    property bool available: true
    property bool optional: false
    property bool showGlyph: true
    readonly property bool stacked: width < 640
    signal clicked
    implicitHeight: Math.max(76, description.y + description.implicitHeight + (stacked ? button.implicitHeight + 32 : 18))
    color: NacreTokens.raised
    radius: 12
    NacreIcon {
        id: glyph
        objectName: "welcomeRowGlyph"
        x: 18
        y: 20
        text: root.icon
        font.pointSize: 19
        color: NacreTokens.accent
        visible: root.icon.length > 0 && root.showGlyph
    }
    NacreText {
        id: title
        x: root.leadingSpace
        y: 16
        width: Math.max(80, root.width - x - (root.stacked ? 18 : button.width + 40))
        text: root.heading
        color: root.optional ? NacreTokens.mutedInk : NacreTokens.ink
        font.pointSize: 13
        wrapMode: Text.Wrap
    }
    NacreText {
        id: description
        x: title.x
        y: title.y + title.implicitHeight + 5
        width: title.width
        text: root.detail
        font.pointSize: 10.5
        color: NacreTokens.mutedInk
        wrapMode: Text.Wrap
    }
    ActionButton {
        id: button
        anchors.right: parent.right
        anchors.rightMargin: 18
        y: root.stacked ? description.y + description.implicitHeight + 12 : (parent.height - height) / 2
        text: root.available ? root.buttonText : NacreI18n.tr("welcome.welcome-row.buttonText.not-installed", "Not installed")
        enabled: root.available
        onClicked: root.clicked()
    }
}
