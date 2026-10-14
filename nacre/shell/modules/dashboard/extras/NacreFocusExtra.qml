import QtQuick
import qs.widgets
import qs.services

NacreExtraFrame {
    id: root
    property int selectedMinutes: NacreDashboardState.preferences.focusMinutes || 25
    readonly property bool started: ["running", "paused"].includes(NacreDashboardState.focusState.status)
    title: NacreI18n.tr("dashboard.registry.focus", "Focus")
    description: NacreI18n.tr("dashboard.focus-extra.description", "Choose a duration, then start your session.")
    icon: "timer"
    NacreSurface {
        anchors.fill: parent
        radius: 16
        color: Qt.tint(NacreTokens.body, Qt.alpha(NacreTokens.accent, .06))
        Column {
            anchors.centerIn: parent
            width: parent.width - 40
            spacing: 18
            NacreText {
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                text: NacreDashboardState.focusState.status === "paused" ? NacreI18n.tr("dashboard.focus-page.status.paused", "Paused") : root.started ? NacreI18n.tr("dashboard.focus-page.status.focus", "Focus") : NacreI18n.tr("dashboard.focus-extra.ready", "Ready when you are")
                color: NacreTokens.mutedInk
                font.pointSize: 11
            }
            NacreText {
                objectName: "focusTimer"
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                text: root.started ? NacreDashboardState.timerText : String(root.selectedMinutes) + ":00"
                font.pointSize: root.width < 680 ? 48 : 72
                font.weight: Font.Light
                color: NacreTokens.accent
            }
            Row {
                anchors.horizontalCenter: parent.horizontalCenter
                spacing: 8
                visible: !root.started
                Repeater {
                    model: [15, 25, 50]
                    ActionButton {
                        required property int modelData
                        objectName: "focusDuration" + modelData
                        text: NacreI18n.tr("ui.focus-page.text.value0-min", "{value0} min", {
                            value0: modelData
                        })
                        compact: true
                        selected: root.selectedMinutes === modelData
                        onClicked: root.selectedMinutes = modelData
                    }
                }
            }
            Row {
                anchors.horizontalCenter: parent.horizontalCenter
                spacing: 8
                ActionButton {
                    id: mainControl
                    objectName: root.started ? "focusPause" : "focusStart"
                    text: !root.started ? NacreI18n.tr("dashboard.focus-extra.start", "Start focus") : NacreDashboardState.running ? NacreI18n.tr("dashboard.focus-extra.pause", "Pause") : NacreI18n.tr("dashboard.focus-extra.resume", "Resume")
                    icon: root.started && NacreDashboardState.running ? "pause" : "play_arrow"
                    selected: true
                    enabled: root.shouldUpdate && NacreDashboardState.busy !== true
                    onClicked: {
                        if (root.started)
                            NacreDashboardState.request(NacreDashboardState.running ? "focus-pause" : "focus-resume");
                        else {
                            NacreDashboardState.save("focusMinutes", root.selectedMinutes);
                            NacreDashboardState.start(root.selectedMinutes);
                        }
                    }
                }
                ActionButton {
                    objectName: "focusStop"
                    visible: root.started
                    text: NacreI18n.tr("dashboard.focus-extra.stop", "Stop")
                    icon: "stop"
                    enabled: root.shouldUpdate && NacreDashboardState.busy !== true
                    onClicked: {
                        mainControl.forceActiveFocus();
                        NacreDashboardState.end();
                    }
                }
            }
        }
        Rectangle {
            x: 22
            y: parent.height - 22
            width: parent.width - 44
            height: 5
            radius: 2.5
            visible: root.started
            color: Qt.alpha(NacreTokens.accent, .12)
            Rectangle {
                width: parent.width * NacreDashboardState.progress
                height: parent.height
                radius: 2.5
                color: NacreTokens.accent
            }
        }
        NacreText {
            x: 22
            y: 14
            width: parent.width - 44
            text: NacreDashboardState.message || ""
            font.pointSize: 10
            color: NacreTokens.mutedInk
            wrapMode: Text.Wrap
        }
    }
}
