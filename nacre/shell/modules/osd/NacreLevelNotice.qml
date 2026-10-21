import QtQuick
import Quickshell
import Quickshell.Wayland
import qs.widgets
import qs.services
import qs.config

NacreWindow {
    id: root
    required property var output
    property bool shown: false
    property string channel: "volume"
    readonly property var model: state
    screen: output
    name: "level-feedback"
    property real reveal: shown ? 1 : 0
    Connections {
        target: NacreTokens
        function onMotionEnabledChanged() {
            if (!NacreTokens.motionEnabled) {
                fadeMotion.stop();
                root.reveal = Qt.binding(() => root.shown ? 1 : 0);
            }
        }
    }
    visible: reveal > 0 && !NacrePanelState.hidden && !WallpaperPlayback.locked
    Behavior on reveal {
        enabled: NacreTokens.motionEnabled
        NumberAnimation {
            id: fadeMotion
            duration: 150
            easing.type: Easing.OutCubic
        }
    }
    anchors.bottom: true
    margins.bottom: NacreFrame.bottom + 24
    implicitWidth: Math.min(336, Math.max(200, output?.width - 48))
    implicitHeight: pill.implicitHeight
    WlrLayershell.exclusionMode: ExclusionMode.Ignore
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
    mask: Region {
        width: 0
        height: 0
    }
    NacreOsdModel {
        id: state
        output: root.output
        channel: root.channel
    }
    NacreOsdPill {
        id: pill
        objectName: "levelFeedbackPill"
        anchors.fill: parent
        opacity: root.reveal
        transform: Translate {
            y: (1 - root.reveal) * -4
        }
        title: state.title
        detail: state.detail
        icon: state.glyph
        value: state.value
        level: state.level
        animateLevel: root.shown && root.reveal >= .99
        muted: state.muted || !state.level && ["capslock", "dnd", "night-light"].includes(root.channel) && !state.enabledState
        valueLabel: !state.level ? "" : state.muted ? NacreI18n.tr("osd.level-notice.text.muted", "Muted") : NacreI18n.tr("osd.feedback.percent", "{value}%", {
            value: NacreI18n.number(Math.round(state.value * 100))
        })
    }
}
