import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import qs.modules.controlcenter as Controls
import "network-groups.js" as Groups
import qs.widgets
import qs.services

Item {
    id: root
    property bool showHeading: true
    property var pendingNetwork: null
    function connectPending() {
        if (!pendingNetwork)
            return false;
        const started = NacreControlActions.connectWifiInline(Object.assign({}, pendingNetwork, {
            ssid: pendingNetwork.ssid || networkName.text.trim(),
            hidden: !pendingNetwork.ssid
        }), password.text);
        password.clear();
        return started;
    }
    function cancelPending() {
        password.clear();
        networkName.clear();
        pendingNetwork = null;
        NacreControlActions.cancelWifiInline();
    }
    Component.onDestruction: {
        NacreControlActions.cancelWifiInline();
    }
    Controls.NacreWifiSaved {
        id: saved
        visible: false
    }
    readonly property var groups: Groups.group(nearby, saved.ssids)
    readonly property var nearby: NacreNetwork.visibleNetworks
    property real maximumListHeight: 210
    Component.onCompleted: NacreNetwork.refresh()
    implicitWidth: 352
    implicitHeight: body.implicitHeight
    function request(action, value) {
        if (NacreControlActions.busy || !NacreNetwork.wifiAvailable)
            return false;
        if (action === "wifi-scan" && !NacreNetwork.wifiEnabled)
            return false;
        if (action === "wifi-disconnect" && (!NacreNetwork.active || !value || value !== NacreNetwork.wifiInterface))
            return false;
        if (!["wifi-radio", "wifi-scan", "wifi-disconnect"].includes(action))
            return false;
        NacreControlActions.request(value === undefined ? [action] : [action, value]);
        return true;
    }
    function activate(point) {
        const current = nearby.find(entry => point?.bssid && entry.bssid === point.bssid && entry.interface === point.interface);
        if (NacreControlActions.busy || !NacreNetwork.wifiAvailable || !NacreNetwork.wifiEnabled || !current)
            return false;
        if (current.active)
            return request("wifi-disconnect", current.interface || NacreNetwork.wifiInterface);
        {
            pendingNetwork = current;
            password.clear();
            Qt.callLater(() => (current.ssid ? password : networkName).forceActiveFocus(Qt.OtherFocusReason));
        }
        return true;
    }
    Column {
        id: body
        width: root.width
        spacing: 10
        RowLayout {
            objectName: "controlWiredConnection"
            width: parent.width
            visible: NacreNetwork.wiredConnected === true
            NacreIcon {
                text: "lan"
                color: NacreTokens.accent
                font.pointSize: 20
            }
            Column {
                Layout.fillWidth: true
                NacreText {
                    text: NacreI18n.tr("controlcenter.network.ethernet", "Ethernet")
                    font.pointSize: 12
                    font.weight: Font.Medium
                }
                NacreText {
                    text: NacreI18n.tr("controlcenter.network.wired-connected", "Connected by cable")
                    font.pointSize: 10
                    color: NacreTokens.mutedInk
                }
            }
        }
        RowLayout {
            width: parent.width
            NacreText {
                Layout.fillWidth: true
                text: NacreI18n.tr("popouts.network-popup.text.wi-fi", "Wi-Fi")
                font.pointSize: 13
                visible: root.showHeading
            }
            ActionButton {
                objectName: "quickWifiScan"
                text: NacreI18n.tr("ui.network-page.text.scan-networks", "Scan networks")
                icon: "refresh"
                compact: true
                enabled: !NacreControlActions.busy && NacreNetwork.wifiAvailable && NacreNetwork.wifiEnabled
                onClicked: root.request("wifi-scan")
            }
            Item {
                Layout.fillWidth: true
                visible: !root.showHeading
            }
            Controls.NacreControlSwitch {
                objectName: "quickWifiPower"
                label: NacreI18n.tr("controlcenter.network.radio", "Wi-Fi radio")
                checked: NacreNetwork.wifiEnabled
                enabled: !NacreControlActions.busy && NacreNetwork.wifiAvailable
                onClicked: if (enabled)
                    root.request("wifi-radio", NacreNetwork.wifiEnabled ? "off" : "on")
            }
        }
        NacreText {
            width: parent.width
            visible: !NacreNetwork.wifiAvailable || !NacreNetwork.wifiEnabled
            text: !NacreNetwork.wifiAvailable ? NacreI18n.tr("ui.network-popup.text.no-wi-fi-adapter-wired-connections-remain-available", "No Wi-Fi adapter; wired connections remain available.") : NacreI18n.tr("popouts.network-popup.text.wi-fi-is-off", "Wi-Fi is off")
            color: NacreTokens.mutedInk
            wrapMode: Text.Wrap
        }
        NacreSurface {
            width: parent.width
            height: credentials.height + 28
            radius: 20
            color: Qt.alpha(NacreTokens.raised, .8)
            visible: !!root.pendingNetwork
            Column {
                id: credentials
                x: 14
                y: 14
                width: parent.width - 28
                spacing: 10
                NacreText {
                    width: parent.width
                    text: NacreI18n.tr("controlcenter.network.join", "Join {name}", {
                        name: root.pendingNetwork?.ssid || ""
                    })
                    font.pointSize: 12
                    elide: Text.ElideMiddle
                }
                NacreText {
                    width: parent.width
                    text: NacreI18n.tr("controlcenter.network.password-hint", "Enter the network password, or leave it empty for an open or saved network.")
                    font.pointSize: 10
                    color: NacreTokens.mutedInk
                    wrapMode: Text.Wrap
                }
                TextField {
                    id: networkName
                    width: parent.width
                    visible: !!root.pendingNetwork && !root.pendingNetwork.ssid
                    enabled: !NacreControlActions.busy
                    placeholderText: NacreI18n.tr("controlcenter.network.name", "Network name")
                    Accessible.name: placeholderText
                    font.family: NacreTokens.textFamily
                    color: NacreTokens.ink
                    placeholderTextColor: NacreTokens.mutedInk
                    maximumLength: 32
                    background: Rectangle {
                        radius: 12
                        color: NacreTokens.body
                        border.width: 1
                        border.color: networkName.activeFocus ? NacreTokens.accent : Qt.alpha(NacreTokens.outline, .4)
                    }
                }
                TextField {
                    id: password
                    objectName: "controlWifiPassword"
                    width: parent.width
                    enabled: !NacreControlActions.busy
                    echoMode: TextInput.Password
                    placeholderText: NacreI18n.tr("controlcenter.network.password", "Password")
                    Accessible.name: placeholderText
                    font.family: NacreTokens.textFamily
                    color: NacreTokens.ink
                    placeholderTextColor: NacreTokens.mutedInk
                    selectByMouse: true
                    onAccepted: root.connectPending()
                    background: Rectangle {
                        radius: 12
                        color: NacreTokens.body
                        border.width: 1
                        border.color: password.activeFocus ? NacreTokens.accent : Qt.alpha(NacreTokens.outline, .4)
                    }
                }
                Row {
                    spacing: 8
                    ActionButton {
                        objectName: "controlWifiConnect"
                        text: NacreControlActions.busy ? NacreI18n.tr("controlcenter.network.connecting", "Connecting…") : NacreI18n.tr("popouts.network-popup.text.connect", "Connect")
                        enabled: !NacreControlActions.busy
                        selected: true
                        onClicked: root.connectPending()
                    }
                    ActionButton {
                        text: NacreI18n.tr("controlcenter.network.cancel", "Cancel")
                        onClicked: root.cancelPending()
                    }
                }
            }
        }
        Connections {
            target: NacreControlActions
            function onBusyChanged() {
                if (!NacreControlActions.busy && !NacreControlActions.message)
                    root.pendingNetwork = null;
            }
        }
        NacreQuickList {
            width: parent.width
            maximumHeight: root.maximumListHeight
            Column {
                width: parent.width
                spacing: 16
                Repeater {
                    model: ["connected", "known", "available"]
                    delegate: Column {
                        id: group
                        required property string modelData
                        readonly property var networks: root.groups[modelData]
                        width: parent.width
                        spacing: 6
                        NacreText {
                            text: group.modelData === "connected" ? NacreI18n.tr("controlcenter.network.connected", "Connected") : group.modelData === "known" ? NacreI18n.tr("controlcenter.network.known", "Known") : NacreI18n.tr("controlcenter.network.available", "Available")
                            font.pointSize: 10
                            font.weight: Font.Medium
                            color: NacreTokens.mutedInk
                        }
                        Repeater {
                            model: group.networks
                            delegate: NacreSurface {
                                id: point
                                required property var modelData
                                width: parent.width
                                height: 56
                                radius: 16
                                color: modelData.active ? NacreColours.palette.m3primaryContainer : NacreTokens.raised
                                readonly property color ink: modelData.active ? NacreColours.palette.m3onPrimaryContainer : NacreTokens.ink
                                NacreIcon {
                                    x: 12
                                    anchors.verticalCenter: parent.verticalCenter
                                    text: point.modelData.strength > 75 ? "signal_wifi_4_bar" : point.modelData.strength > 45 ? "network_wifi_3_bar" : "network_wifi_1_bar"
                                    color: point.ink
                                    font.pointSize: 17
                                }
                                NacreText {
                                    x: 42
                                    width: Math.max(0, parent.width - connectionButton.width - 68)
                                    height: parent.height
                                    text: point.modelData.ssid || NacreI18n.tr("popouts.network-popup.text.hidden-network", "Hidden network")
                                    font.pointSize: 11
                                    color: point.ink
                                    verticalAlignment: Text.AlignVCenter
                                    elide: Text.ElideRight
                                }
                                NacreIcon {
                                    anchors.right: connectionButton.left
                                    anchors.rightMargin: 5
                                    anchors.verticalCenter: parent.verticalCenter
                                    text: "lock"
                                    visible: !!point.modelData.security && point.modelData.security !== "--"
                                    font.pointSize: 11
                                    color: point.ink
                                }
                                ActionButton {
                                    id: connectionButton
                                    objectName: point.modelData.active ? "connectedWifiAction" : "availableWifiAction"
                                    anchors.right: parent.right
                                    anchors.rightMargin: 6
                                    anchors.verticalCenter: parent.verticalCenter
                                    text: point.modelData.active ? NacreI18n.tr("popouts.network-popup.text.disconnect", "Disconnect") : NacreI18n.tr("popouts.network-popup.text.connect", "Connect")
                                    compact: true
                                    accessibleLabel: NacreI18n.tr("controlcenter.network.device-action", "{action} {name}", {
                                        action: text,
                                        name: point.modelData.ssid || NacreI18n.tr("popouts.network-popup.text.hidden-network", "Hidden network")
                                    })
                                    enabled: !NacreControlActions.busy && NacreNetwork.wifiAvailable && NacreNetwork.wifiEnabled
                                    onClicked: root.activate(point.modelData)
                                }
                            }
                        }
                        NacreText {
                            width: parent.width
                            visible: group.networks.length === 0
                            text: group.modelData === "connected" ? NacreI18n.tr("controlcenter.control-center.detail.not-connected", "Not connected") : group.modelData === "known" ? NacreI18n.tr("controlcenter.network.none-known", "No saved networks nearby") : NacreI18n.tr("popouts.network-popup.text.no-nearby-networks", "No nearby networks")
                            color: NacreTokens.mutedInk
                            font.pointSize: 9
                        }
                    }
                }
            }
        }
        NacreText {
            width: parent.width
            visible: root.nearby.length === 0 && NacreNetwork.wifiEnabled
            text: NacreNetwork.busy ? NacreI18n.tr("popouts.network-popup.text.reading-wi-fi-state", "Reading Wi-Fi state…") : NacreI18n.tr("popouts.network-popup.text.no-nearby-networks", "No nearby networks")
            color: NacreTokens.mutedInk
        }
        NacreText {
            width: parent.width
            visible: text !== ""
            text: NacreNetwork.error || NacreNetwork.monitorError || NacreControlActions.message || ""
            maximumLineCount: 3
            elide: Text.ElideRight
            wrapMode: Text.WordWrap
            color: NacreTokens.mutedInk
        }
    }
}
