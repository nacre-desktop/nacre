import QtQuick
import Quickshell
import Quickshell.Wayland
import qs.widgets
import qs.config
import qs.services

NacreWindow {
    id: root
    required property var output
    property string text: ""
    property real center: 0
    property bool ready: false
    screen: output
    name: "status-hint"
    anchors.top: true
    anchors.left: true
    implicitWidth: label.implicitWidth + 24
    implicitHeight: 32
    margins.top: NacreFrame.headerHeight + 8
    margins.left: Math.max(8, Math.min((output?.width ?? 800) - implicitWidth - 8, center - implicitWidth / 2))
    visible: ready && text !== "" && !NacrePanelState.hidden && DesktopSettings.data.topEdge !== false
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.exclusionMode: ExclusionMode.Ignore
    WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
    mask: Region {
        width: 0
        height: 0
    }
    onTextChanged: {
        ready = false;
        if (text)
            delay.restart();
        else
            delay.stop();
    }
    Timer {
        id: delay
        interval: 300
        onTriggered: root.ready = true
    }
    NacreSurface {
        anchors.fill: parent
        radius: 12
        color: NacreTokens.body
        border.width: 1
        border.color: Qt.alpha(NacreTokens.outline, .4)
        NacreText {
            id: label
            anchors.centerIn: parent
            text: root.text
            font.pointSize: 10
        }
    }
}
