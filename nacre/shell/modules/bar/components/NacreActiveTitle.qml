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
    readonly property var client: NacreCompositor.overviewHoverWindow || NacreCompositor.overviewSelectedWindow || WindowTitle.displayWindow(NacreCompositor.activeWindow, NacreCompositor.windows, NacreCompositor.activeWorkspaceId, previousWindowId, NacreCompositor.backendName)
    readonly property string displayTitle: NacreCompositor.backendName === "niri" && NacreCompositor.overviewOpen && !NacreCompositor.overviewHoverSupported ? NacreI18n.tr("ui.compositor.hover-title-unavailable", "Hover titles require the niri overview companion") : client ? NacreI18n.windowCaption(WindowTitle.caption(client.title || client.wmClass, client.wmClass, ChatWindowTitle.titles[client.pid]), client.wmClass) : NacreI18n.tr("desktop.title.empty", "Desktop")
    readonly property string spinnerPrefix: client?.wmClass === "nacre-ai-task" ? /^[\u2800-\u28ff]\s*/.exec(displayTitle)?.[0] || "" : ""
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
    implicitWidth: glyph.width + spinner.width + caption.implicitWidth + row.spacing
    implicitHeight: 30
    Accessible.role: Accessible.StaticText
    Accessible.name: displayTitle
    Item {
        id: row
        anchors.centerIn: parent
        readonly property real spacing: NacreAppearance.spacing.small
        width: Math.min(root.width, root.implicitWidth)
        height: Math.max(glyph.height, caption.implicitHeight)
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
        Item {
            id: spinner
            x: glyph.width + row.spacing
            width: root.spinnerPrefix ? spinnerAdvance.advanceWidth : 0
            height: glyph.height
            anchors.verticalCenter: glyph.verticalCenter
            visible: root.spinnerPrefix.length > 0 && caption.width > 0
            NacreText {
                id: spinnerText
                objectName: "nacreActiveTitleSpinner"
                text: root.spinnerPrefix.charAt(0)
                font: caption.font
                color: root.colour
                // Center the complete six-dot animation grid, rather than the
                // font line box or each frame's changing occupied dots.
                y: (parent.height - spinnerGrid.tightBoundingRect.height) / 2 - baselineOffset - spinnerGrid.tightBoundingRect.y
                Accessible.ignored: true
            }
            TextMetrics {
                id: spinnerGrid
                font: spinnerText.font
                renderType: spinnerText.renderType
                text: "⠿"
            }
            TextMetrics {
                id: spinnerAdvance
                font: spinnerText.font
                renderType: spinnerText.renderType
                text: root.spinnerPrefix
            }
        }
        NacreText {
            id: caption
            objectName: "nacreActiveTitle"
            x: spinner.x + spinner.width
            width: Math.max(0, row.width - x)
            anchors.verticalCenter: parent.verticalCenter
            text: root.displayTitle.slice(root.spinnerPrefix.length)
            font.family: NacreTokens.textFamily
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
