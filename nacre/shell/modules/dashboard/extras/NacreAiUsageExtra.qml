import QtQuick
import Quickshell.Io
import qs.utils
import qs.widgets
import qs.services

NacreExtraFrame {
    id: root
    property var codexRows: []
    property var claudeRows: []
    property bool readingClaude: false
    property string error: ""
    title: NacreI18n.tr("dashboard.registry.ai-usage", "AI usage")
    description: NacreI18n.tr("dashboard.ai-extra.description", "Provider-reported limits for your default Codex and Claude accounts.")
    icon: "data_usage"
    function refresh() {
        if (usage.running || !shouldUpdate)
            return;
        error = "";
        readingClaude = false;
        usage.command = [NacrePaths.userBin + "/nacre-ai-usage", "read", "--agent", "codex"];
        usage.running = true;
    }
    Component.onCompleted: refresh()
    Process {
        id: usage
        stdout: StdioCollector {
            onStreamFinished: {
                try {
                    const rows = JSON.parse(text).rows || [];
                    if (root.readingClaude)
                        root.claudeRows = rows;
                    else
                        root.codexRows = rows;
                } catch (error) {
                    root.error = NacreI18n.tr("dashboard.ai-extra.read-error", "Could not read the usage report. Try Refresh.");
                }
            }
        }
        onExited: {
            if (!root.readingClaude && root.shouldUpdate) {
                root.readingClaude = true;
                command = [NacrePaths.userBin + "/nacre-ai-usage", "read", "--agent", "claude"];
                running = true;
            }
        }
    }
    Column {
        anchors.fill: parent
        spacing: 14
        Row {
            width: parent.width
            spacing: 14
            Repeater {
                model: [
                    {
                        name: NacreI18n.tr("dashboard.ai-extra.codex", "Codex"),
                        rows: root.codexRows
                    },
                    {
                        name: NacreI18n.tr("dashboard.ai-extra.claude", "Claude"),
                        rows: root.claudeRows
                    }
                ]
                NacreSurface {
                    required property var modelData
                    width: (parent.width - 14) / 2
                    height: Math.min(340, root.height - 180)
                    radius: 16
                    color: NacreTokens.body
                    Column {
                        x: 18
                        y: 18
                        width: parent.width - 36
                        spacing: 14
                        NacreText {
                            text: parent.parent.modelData.name
                            font.pointSize: 24
                            font.weight: Font.DemiBold
                            color: NacreTokens.accent
                        }
                        Repeater {
                            model: parent.parent.modelData.rows.slice(1, 6)
                            NacreText {
                                required property string modelData
                                width: parent.width
                                text: modelData
                                font.pointSize: 11
                                wrapMode: Text.Wrap
                                maximumLineCount: 3
                                elide: Text.ElideRight
                            }
                        }
                        NacreText {
                            visible: !parent.parent.modelData.rows.length
                            width: parent.width
                            text: usage.running ? NacreI18n.tr("dashboard.ai-extra.reading", "Reading usage…") : NacreI18n.tr("dashboard.ai-extra.unavailable", "Usage unavailable")
                            font.pointSize: 11
                            color: NacreTokens.mutedInk
                        }
                    }
                }
            }
        }
        NacreText {
            width: parent.width
            text: root.error
            visible: !!text
            font.pointSize: 10
            color: NacreTokens.mutedInk
            wrapMode: Text.Wrap
        }
        ActionButton {
            text: NacreI18n.tr("dashboard.ai-extra.refresh", "Refresh usage")
            compact: true
            enabled: !usage.running
            onClicked: root.refresh()
        }
    }
}
