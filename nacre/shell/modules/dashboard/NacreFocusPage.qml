import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import qs.widgets
import qs.services

Flickable {
    id: root
    required property bool shouldUpdate
    property string search: ""
    implicitWidth: 940
    implicitHeight: 720
    contentWidth: width
    contentHeight: sections.height
    clip: true
    boundsBehavior: Flickable.StopAtBounds
    ScrollBar.vertical: NacreScrollBar {}
    FastScroll {
        view: root
    }
    Column {
        id: sections
        width: root.width
        spacing: 18
        NacreDashboardHero {
            width: parent.width
            height: 230
            NacreChamberGauge {
                x: 18
                y: 18
                width: 210
                height: 190
                value: NacreDashboardState.progress
                label: NacreDashboardState.focusState.status === "paused" ? "Paused" : "Focus"
                detail: "Time to do one thing"
            }
            Column {
                x: root.width < 650 ? 240 : 290
                y: 26
                width: parent.width - x - 20
                spacing: 14
                NacreText {
                    text: ["running", "paused"].includes(NacreDashboardState.focusState.status) ? NacreDashboardState.timerText : "25:00"
                    font.pointSize: 36
                    font.weight: Font.Light
                    color: NacreTokens.accent
                }
                NacreText {
                    width: parent.width
                    text: "Quiet notifications. A little time for what matters."
                    font.pointSize: 11
                    color: NacreTokens.mutedInk
                    wrapMode: Text.Wrap
                }
                Flow {
                    width: parent.width
                    spacing: 8
                    Repeater {
                        model: [15, 25, 50]
                        ActionButton {
                            required property int modelData
                            text: modelData + " min"
                            compact: true
                            visible: !["running", "paused"].includes(NacreDashboardState.focusState.status)
                            onClicked: NacreDashboardState.start(modelData)
                        }
                    }
                    ActionButton {
                        text: NacreDashboardState.running ? "Pause" : "Resume"
                        compact: true
                        visible: ["running", "paused"].includes(NacreDashboardState.focusState.status)
                        onClicked: NacreDashboardState.request(NacreDashboardState.running ? "focus-pause" : "focus-resume")
                    }
                    ActionButton {
                        text: "Stop"
                        compact: true
                        visible: ["running", "paused"].includes(NacreDashboardState.focusState.status)
                        onClicked: NacreDashboardState.end(false)
                    }
                }
            }
        }
        NacreText {
            text: "Quick capture"
            font.pointSize: 15
            color: NacreTokens.accent
        }
        NacreTextField {
            id: note
            objectName: "focusCapture"
            width: parent.width
            height: 44
            placeholderText: "A thought for your Brain…"
            enabled: NacreDashboardState.brain.available
            onAccepted: if (text.trim()) {
                NacreDashboardState.request("capture", text.trim());
                text = "";
            }
        }
        NacreText {
            width: parent.width
            text: NacreDashboardState.brain.available ? "Enter saves a private note to your Brain vault." : "Brain is optional. Set it up to capture and browse notes; the focus timer works on its own."
            font.pointSize: 10
            color: NacreTokens.mutedInk
            wrapMode: Text.Wrap
        }
        NacreTextField {
            width: parent.width
            height: 38
            placeholderText: "Filter recent notes and tasks"
            onTextChanged: root.search = text.trim().toLowerCase()
        }
        GridLayout {
            width: parent.width
            columns: root.width < 650 ? 1 : 2
            columnSpacing: 22
            rowSpacing: 16
            Column {
                Layout.fillWidth: true
                Layout.preferredWidth: root.width / 2
                Layout.alignment: Qt.AlignTop
                spacing: 12
                NacreText {
                    text: "Open tasks · recent notes"
                    font.pointSize: 15
                    color: NacreTokens.accent
                }
                Repeater {
                    model: (NacreDashboardState.brain.tasks || []).filter(item => !root.search || item.text.toLowerCase().includes(root.search))
                    ActionButton {
                        required property var modelData
                        width: parent.width
                        text: modelData.text.slice(0, 70)
                        compact: true
                        onClicked: NacreDashboardState.openNote(modelData.path)
                    }
                }
                NacreText {
                    width: parent.width
                    text: "Tasks are read from your most recent notes."
                    font.pointSize: 9
                    color: NacreTokens.mutedInk
                    wrapMode: Text.Wrap
                }
            }
            Column {
                Layout.fillWidth: true
                Layout.preferredWidth: root.width / 2
                Layout.alignment: Qt.AlignTop
                spacing: 12
                NacreText {
                    text: "Recent notes"
                    font.pointSize: 15
                    color: NacreTokens.accent
                }
                Repeater {
                    model: (NacreDashboardState.brain.recent || []).filter(item => !root.search || item.title.toLowerCase().includes(root.search))
                    ActionButton {
                        required property var modelData
                        width: parent.width
                        text: modelData.title.slice(0, 70)
                        compact: true
                        onClicked: NacreDashboardState.openNote(modelData.path)
                    }
                }
            }
        }
        NacreText {
            width: parent.width
            text: NacreDashboardState.message
            visible: text !== ""
            font.pointSize: 10
            color: NacreTokens.mutedInk
            wrapMode: Text.Wrap
        }
    }
}
