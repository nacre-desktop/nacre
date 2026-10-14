import QtQuick
import qs.modules.dashboard as Dashboard
import qs.modules.controlcenter as Controls

Loader {
    id: root
    required property string editor
    property var screen
    sourceComponent: editor === "dashboard" ? dashboard : controls
    Component {
        id: dashboard
        Dashboard.NacreDashboardCustomize {
            height: contentHeight
            interactive: false
        }
    }
    Component {
        id: controls
        Controls.NacreControlEdit {
            screen: root.screen
        }
    }
}
