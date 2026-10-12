import QtQuick
import qs.widgets
import qs.services

Row {
    id: root
    required property string kind
    required property var host
    property string screenName: NacrePanelState.activeName()
    visible: NacrePearls.enabled
    spacing: 4
    height: 34
    NacreTearHandle {
        kind: root.kind
        host: root.host
        screenName: root.screenName
    }
    ActionButton {
        objectName: "detachPanel"
        text: ""
        icon: "open_in_new"
        compact: true
        accessibleLabel: "Open panel in window"
        enabled: root.host?.canDetach !== false
        onClicked: NacrePearls.detach(root.kind, root.host, root.screenName)
    }
}
