pragma Singleton
import Quickshell

// External configuration compatibility; collection belongs to NacreNetwork.
Singleton {
    readonly property var networks: NacreNetwork.networks
    readonly property var visibleNetworks: NacreNetwork.visibleNetworks
    readonly property var active: NacreNetwork.active
    readonly property bool wifiEnabled: NacreNetwork.wifiEnabled
    readonly property string wifiInterface: NacreNetwork.wifiInterface
    function refresh() {
        NacreNetwork.refresh();
    }
}
