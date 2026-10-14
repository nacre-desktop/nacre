import QtQuick
import qs.modules.dashboard as Dashboard
import qs.modules.controlcenter as Controls

Loader {
    id: root
    required property string editor
    property string section: "all"
    readonly property bool hasContent: editor !== "control-center" || section === "layout" || section === "all" || (section === "detected" ? (item?.detectedEntries?.length || 0) : (item?.availableEntries?.length || 0)) > 0
    property var screen
    sourceComponent: editor === "dashboard" ? dashboard : controls
    Component {
        id: dashboard
        Dashboard.NacreDashboardCustomize {
            section: root.section
            showHeading: false
            showDescription: false
            height: contentHeight
            interactive: false
        }
    }
    Component {
        id: controls
        Controls.NacreControlEdit {
            screen: root.screen
            section: root.section
            showHeadings: false
            showHint: false
            showReset: false
            settingsRows: true
        }
    }
}
