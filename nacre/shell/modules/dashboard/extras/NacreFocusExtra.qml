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
    color: NacreTokens.body
    Item {
        anchors.fill: parent
        readonly property real ringSize: Math.max(90, Math.min(root.width < 680 ? 180 : 250, height - 132))
        Column {
            anchors.centerIn: parent
            width: parent.width - 40
            spacing: parent.height < 320 ? 8 : 18
            NacreText {
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                text: NacreDashboardState.focusState.status === "paused" ? NacreI18n.tr("dashboard.focus-page.status.paused", "Paused") : root.started ? NacreI18n.tr("dashboard.focus-page.status.focus", "Focus") : NacreI18n.tr("dashboard.focus-extra.ready", "Ready when you are")
                color: NacreTokens.mutedInk
                font.pointSize: 11
            }
            Item {
                width: parent.parent.ringSize
                height: width
                anchors.horizontalCenter: parent.horizontalCenter
                NacreSurface {
                    anchors.fill: parent
                    radius: width / 2
                    color: Qt.alpha(NacreTokens.accent, .035)
                }
                Canvas {
                    id: dial
                    objectName: "focusDial"
                    anchors.fill: parent
                    property real progress: root.started ? NacreDashboardState.progress : 0
                    property color accent: NacreTokens.accent
                    onProgressChanged: requestPaint()
                    onAccentChanged: requestPaint()
                    onWidthChanged: requestPaint()
                    onAvailableChanged: if (available)
                        requestPaint()
                    onPaint: {
                        const ctx = getContext("2d");
                        ctx.reset();
                        const c = width / 2, r = c - 5;
                        ctx.lineWidth = 4;
                        ctx.lineCap = "round";
                        ctx.strokeStyle = Qt.alpha(accent, .16);
                        ctx.beginPath();
                        ctx.arc(c, c, r, 0, Math.PI * 2);
                        ctx.stroke();
                        if (progress > 0) {
                            ctx.strokeStyle = accent;
                            ctx.beginPath();
                            ctx.arc(c, c, r, -Math.PI / 2, -Math.PI / 2 + Math.PI * 2 * progress);
                            ctx.stroke();
                        }
                    }
                }
                NacreText {
                    objectName: "focusTimer"
                    anchors.centerIn: parent
                    text: root.started ? NacreDashboardState.timerText : String(root.selectedMinutes) + ":00"
                    font.pointSize: Math.max(12, Math.min(root.width < 680 ? 38 : 52, parent.width * .21))
                    font.weight: Font.Light
                    color: NacreTokens.accent
                }
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
