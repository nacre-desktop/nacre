import QtQuick
import qs.widgets
import qs.services
import qs.config
import qs.utils

Item {
    id: root
    required property var monitor
    property bool horizontal: false
    property color colour: NacreColours.palette.m3primary
    readonly property Item child: caption
    readonly property var client: NacreHyprland.activeClient
    readonly property string displayTitle: String(client?.title || client?.wmClass || "Desktop")
    implicitWidth: glyph.width + caption.implicitWidth + row.spacing
    implicitHeight: 30
    Accessible.role: Accessible.StaticText
    Accessible.name: displayTitle
    Row {
        id: row
        anchors.centerIn: parent
        spacing: NacreAppearance.spacing.small
        width: Math.min(root.width, root.implicitWidth)
        NacreIcon {
            id: glyph
            width: 24
            height: 24
            clip: true
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
            text: root.client ? NacreIcons.getAppCategoryIcon(root.client.wmClass, "desktop_windows") : "desktop_windows"
            color: root.colour
            anchors.verticalCenter: parent.verticalCenter
        }
        NacreText {
            id: caption
            objectName: "nacreActiveTitle"
            width: Math.max(0, row.width - glyph.width - row.spacing)
            anchors.verticalCenter: parent.verticalCenter
            text: root.displayTitle
            font.family: NacreAppearance.font.family.mono
            font.pointSize: NacreAppearance.font.size.smaller
            color: root.colour
            elide: Text.ElideRight
            maximumLineCount: 1
        }
    }
    function adjustVolume(delta) {
        if (Number.isFinite(delta) && Number.isFinite(NacreAudio.volume))
            NacreAudio.setVolume(Math.max(0, Math.min(1, NacreAudio.volume + Math.sign(delta) * 0.05)));
    }
    MouseArea {
        anchors.fill: parent
        acceptedButtons: Qt.NoButton
        onWheel: wheel => {
            if (wheel.angleDelta.y !== 0) {
                root.adjustVolume(wheel.angleDelta.y);
                wheel.accepted = true;
            } else
                wheel.accepted = false;
        }
    }
}
