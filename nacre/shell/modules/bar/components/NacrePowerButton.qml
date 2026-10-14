import QtQuick
import qs.widgets
import qs.services

NacreSurface {
    id: root
    implicitWidth: 30
    implicitHeight: 30
    radius: width / 2
    color: "transparent"
    function toggle() {
        NacrePanelState.toggleSession();
    }
    NacreHoverCue {
        anchors.fill: parent
        ink: NacreColours.palette.m3error
        progress: input.hovered || input.keyboardFocusVisible ? 1 : 0
    }
    NacreIcon {
        anchors.centerIn: parent
        text: "power_settings_new"
        color: NacreColours.palette.m3error
    }
    NacreInteraction {
        id: input
        objectName: "nacrePowerActivation"
        accessibleName: NacreI18n.tr("components.power-button.accessibleName.open-power-menu", "Open power menu")
        function onClicked() {
            root.toggle();
        }
    }
}
