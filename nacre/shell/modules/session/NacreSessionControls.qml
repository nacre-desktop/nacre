import QtQuick
import qs.widgets
import qs.services

Item {
    id: root
    required property var visibilities
    readonly property var actions: [
        {
            key: "lock",
            label: "Lock",
            icon: "lock",
            command: ["loginctl", "lock-session"]
        },
        {
            key: "logout",
            label: "Log out",
            icon: "logout",
            command: ["hyprctl", "eval", "hl.dispatch(hl.dsp.exit())"]
        },
        {
            key: "restart",
            label: "Restart",
            icon: "restart_alt",
            command: ["systemctl", "reboot"]
        },
        {
            key: "poweroff",
            label: "Power off",
            icon: "power_settings_new",
            command: ["systemctl", "poweroff"]
        }
    ]
    implicitWidth: 150
    implicitHeight: 380
    function activate(key) {
        const action = actions.find(entry => entry.key === key);
        if (!action || !visibilities.session)
            return false;
        visibilities.session = false;
        AppLaunch.run(action.command);
        return true;
    }
    Column {
        x: 15
        y: 12
        spacing: 8
        Repeater {
            model: root.actions
            delegate: NacreSurface {
                required property var modelData
                width: 120
                height: 80
                radius: 18
                color: NacreColours.palette.m3surfaceContainer
                NacreIcon {
                    y: 12
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: parent.modelData.icon
                    color: parent.modelData.key === "poweroff" ? NacreColours.palette.m3error : NacreColours.palette.m3onSurface
                }
                NacreText {
                    y: 47
                    width: parent.width
                    text: parent.modelData.label
                    horizontalAlignment: Text.AlignHCenter
                    font.pointSize: 11
                }
                NacreInteraction {
                    accessibleName: parent.modelData.label
                    function onClicked() {
                        root.activate(parent.modelData.key);
                    }
                }
            }
        }
    }
}
