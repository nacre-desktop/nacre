import QtQuick
import QtQuick.Controls.Basic
import qs.widgets
import qs.services

NacreExtraFrame {
    id: root
    title: NacreI18n.tr("dashboard.registry.focus", "Focus")
    description: NacreI18n.tr("dashboard.focus-extra.description", "A little time for one thing. Keep the next thought safely in your Brain.")
    icon: "timer"
    Column {
        anchors.fill: parent
        spacing: 16
        NacreSurface {
            width: parent.width
            height: Math.min(250, root.height * .46)
            radius: 16
            color: Qt.tint(NacreTokens.body, Qt.alpha(NacreTokens.accent, .08))
            NacreText {
                x: 22
                y: 16
                text: NacreDashboardState.focusState.status === "paused" ? NacreI18n.tr("dashboard.focus-page.status.paused", "Paused") : NacreI18n.tr("dashboard.focus-page.status.focus", "Focus")
                color: NacreTokens.mutedInk
                font.pointSize: 11
            }
            NacreText {
                x: 22
                y: 46
                text: ["running", "paused"].includes(NacreDashboardState.focusState.status) ? NacreDashboardState.timerText : String(NacreDashboardState.preferences.focusMinutes || 25) + ":00"
                font.pointSize: 58
                font.weight: Font.Light
                color: NacreTokens.accent
            }
            Rectangle {
                x: 22
                y: parent.height - 70
                width: parent.width - 44
                height: 5
                radius: 2.5
                color: Qt.alpha(NacreTokens.accent, .12)
                Rectangle {
                    width: parent.width * NacreDashboardState.progress
                    height: parent.height
                    radius: 2.5
                    color: NacreTokens.accent
                }
            }
            Row {
                x: 22
                anchors.bottom: parent.bottom
                anchors.bottomMargin: 16
                spacing: 8
                Repeater {
                    model: ["running", "paused"].includes(NacreDashboardState.focusState.status) ? [] : [15, 25, 50]
                    ActionButton {
                        required property int modelData
                        text: NacreI18n.tr("ui.focus-page.text.value0-min", "{value0} min", {
                            value0: modelData
                        })
                        compact: true
                        onClicked: NacreDashboardState.start(modelData)
                    }
                }
                ActionButton {
                    visible: ["running", "paused"].includes(NacreDashboardState.focusState.status)
                    text: NacreDashboardState.running ? NacreI18n.tr("dashboard.focus-extra.pause", "Pause") : NacreI18n.tr("dashboard.focus-extra.resume", "Resume")
                    compact: true
                    onClicked: NacreDashboardState.request(NacreDashboardState.running ? "focus-pause" : "focus-resume")
                }
                ActionButton {
                    visible: ["running", "paused"].includes(NacreDashboardState.focusState.status)
                    text: NacreI18n.tr("dashboard.focus-extra.stop", "Stop")
                    compact: true
                    onClicked: NacreDashboardState.end()
                }
            }
        }
        NacreText {
            text: NacreI18n.tr("dashboard.focus-extra.capture", "Quick capture")
            font.pointSize: 13
            font.weight: Font.DemiBold
        }
        Row {
            width: parent.width
            spacing: 8
            NacreTextField {
                id: capture
                width: parent.width - save.width - 8
                height: 40
                placeholderText: NacreI18n.tr("dashboard.focus-extra.thought", "A thought worth keeping")
            }
            ActionButton {
                id: save
                text: NacreI18n.tr("dashboard.focus-extra.save", "Save")
                icon: "save"
                compact: true
                enabled: !!capture.text.trim() && !DesktopExtras.busy.capture
                onClicked: DesktopExtras.request("capture", {
                    text: capture.text
                })
            }
        }
        NacreText {
            width: parent.width
            text: DesktopExtras.message || DesktopExtras.captured || NacreDashboardState.message
            font.pointSize: 10
            color: NacreTokens.mutedInk
            wrapMode: Text.Wrap
        }
    }
    Connections {
        target: DesktopExtras
        function onCapturedChanged() {
            if (DesktopExtras.captured)
                capture.text = "";
        }
    }
}
