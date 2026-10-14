import QtQuick
import qs.widgets
import qs.services
import qs.modules.bar.popouts as Connectivity

NacreSettingsPage {
    id: root
    property string page: "network"
    function request(action, value) {
        return controls.request(action, value);
    }
    function connect(network) {
        return controls.activate(network);
    }
    NacreSettingsSection {
        title: NacreI18n.tr("ui.network-page.title.wi-fi", "Wi-Fi")
        Connectivity.NacreNetworkPopup {
            id: controls
            showHeading: false
            objectName: "settingsWifiControls"
            width: parent.width
            maximumListHeight: 420
        }
        ActionButton {
            text: NacreI18n.tr("ui.network-page.text.connection-editor", "Connection editor")
            onClicked: DesktopActions.execute("network")
        }
    }
}
