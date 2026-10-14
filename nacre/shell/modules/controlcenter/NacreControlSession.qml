import QtQuick
import QtQuick.Layouts
import qs.widgets
import qs.services

Column {
    id: root
    property bool active: true
    property string pendingKey: ""
    readonly property var actions: [
        {
            key: "lock",
            label: NacreI18n.tr("controlcenter.footer.lock", "Lock screen"),
            icon: "lock"
        },
        {
            key: "logout",
            label: NacreI18n.tr("session.session-controls.label.log-out", "Log out"),
            icon: "logout"
        },
        {
            key: "restart",
            label: NacreI18n.tr("session.session-controls.label.restart", "Restart"),
            icon: "restart_alt"
        },
        {
            key: "poweroff",
            label: NacreI18n.tr("session.session-controls.label.power-off", "Power off"),
            icon: "power_settings_new"
        }
    ]
    readonly property var pending: actions.find(action => action.key === pendingKey) || null
    spacing: 18
    function choose(key) {
        if (!active || !actions.some(action => action.key === key))
            return false;
        if (key === "lock") {
            DesktopActions.execute("lock");
            NacrePanelState.close();
        } else
            pendingKey = key;
        return true;
    }
    function confirm() {
        if (!active || !pending)
            return false;
        const key = pendingKey;
        pendingKey = "";
        if (key === "logout")
            NacreCompositor.logout();
        else if (key === "restart" || key === "poweroff")
            AppLaunch.run(["systemctl", key === "restart" ? "reboot" : "poweroff"]);
        NacrePanelState.close();
        return true;
    }
    onActiveChanged: if (!active)
        pendingKey = ""
    GridLayout {
        width: parent.width
        columns: 2
        columnSpacing: 10
        rowSpacing: 10
        Repeater {
            model: root.actions
            delegate: NacreControlTile {
                required property var modelData
                objectName: "controlSession-" + modelData.key
                Layout.fillWidth: true
                label: modelData.label
                icon: modelData.icon
                available: root.active
                selected: root.pendingKey === modelData.key
                onClicked: root.choose(modelData.key)
            }
        }
    }
    NacreSurface {
        objectName: "controlSessionConfirmation"
        width: parent.width
        implicitHeight: confirmation.implicitHeight + 28
        visible: !!root.pending
        radius: 22
        color: NacreTokens.raised
        Column {
            id: confirmation
            x: 14
            y: 14
            width: parent.width - 28
            spacing: 14
            NacreText {
                width: parent.width
                text: NacreI18n.tr("controlcenter.session.confirm", "{action}?", {
                    action: root.pending?.label || ""
                })
                font.pointSize: 15
                wrapMode: Text.WordWrap
            }
            RowLayout {
                width: parent.width
                spacing: 8
                ActionButton {
                    objectName: "controlSessionCancel"
                    Layout.fillWidth: true
                    text: NacreI18n.tr("controlcenter.session.cancel", "Cancel")
                    onClicked: root.pendingKey = ""
                }
                ActionButton {
                    objectName: "controlSessionConfirm"
                    Layout.fillWidth: true
                    text: root.pending?.label || ""
                    selected: true
                    onClicked: root.confirm()
                }
            }
        }
    }
}
