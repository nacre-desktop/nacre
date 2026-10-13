import QtQuick
import Quickshell.Io
import qs.widgets
import qs.services
import qs.config
import qs.utils
import "../../../utils/scripts/window-title.js" as WindowTitle

Item {
    id: root
    required property var monitor
    property bool horizontal: false
    property color colour: NacreColours.palette.m3primary
    readonly property Item child: caption
    property string previousWindowId: ""
    readonly property var client: WindowTitle.displayWindow(NacreCompositor.activeWindow, NacreCompositor.windows, NacreCompositor.activeWorkspaceId, previousWindowId, NacreCompositor.backendName)
    readonly property string displayTitle: client ? NacreI18n.windowCaption(WindowTitle.caption(client.title || client.wmClass, client.wmClass, ChatWindowTitle.titles[client.pid]), client.wmClass) : NacreI18n.tr("desktop.title.empty", "Desktop")
    function rememberWindow() {
        if (NacreCompositor.activeWindow?.windowId)
            previousWindowId = NacreCompositor.activeWindow.windowId;
        else if (!previousWindowId && NacreCompositor.backendName === "niri")
            previousWindowId = NacreCompositor.activeWorkspace?.activeWindowId || "";
    }
    IpcHandler {
        target: "windowTitle-" + (root.monitor?.name || "default")
        function state(): string {
            return JSON.stringify({
                title: root.displayTitle,
                retained: !NacreCompositor.activeWindow && !!root.client
            });
        }
    }
    Component.onCompleted: rememberWindow()
    Connections {
        target: NacreCompositor
        function onActiveWindowChanged() {
            root.rememberWindow();
        }
    }
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
