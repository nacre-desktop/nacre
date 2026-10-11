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
        const view = NacrePanelState.getForActive();
        if (!view)
            return;
        const opening = !view.session;
        view.launcher = false;
        view.dashboard = false;
        view.osd = false;
        for (const panel of Object.values(NacrePanelState.panels)) {
            panel.popouts.hasCurrent = false;
            panel.popouts.pinned = false;
        }
        view.session = opening;
    }
    NacreIcon {
        anchors.centerIn: parent
        text: "power_settings_new"
        color: NacreColours.palette.m3error
    }
    NacreInteraction {
        objectName: "nacrePowerActivation"
        accessibleName: "Open power menu"
        function onClicked() {
            root.toggle();
        }
    }
}
