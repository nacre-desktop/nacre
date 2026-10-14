import QtQuick
import QtQuick.Layouts
import qs.widgets
import qs.services

Column {
    id: root
    required property var screen
    property bool active: true
    signal detailsRequested(string section, bool pointerOwned)
    property bool pointerActivation: false
    readonly property var tileIds: NacreControlPreferences.tiles.filter(id => catalog.supported(id))
    function fullWidth(id) {
        return ["volume", "brightness", "mic-volume", "keyboard", "appearance"].includes(id);
    }
    function span(index) {
        if (fullWidth(tileIds[index]))
            return 2;
        let count = 0;
        for (let before = index - 1; before >= 0 && !fullWidth(tileIds[before]); before--)
            count++;
        return count % 2 === 0 && (index === tileIds.length - 1 || fullWidth(tileIds[index + 1])) ? 2 : 1;
    }
    function toggle(id) {
        catalog.toggle(id);
    }
    NacreControlCatalog {
        id: catalog
        screen: root.screen
        onDetailsRequested: section => root.detailsRequested(section === "dnd" ? "notifications" : section, root.pointerActivation)
    }
    Component.onCompleted: if (NacreControlPreferences.optionalTiles.includes("vpn"))
        NacreVpn.refresh()
    spacing: 18
    GridLayout {
        width: parent.width
        columns: 2
        columnSpacing: 8
        rowSpacing: 8
        Repeater {
            model: root.tileIds
            delegate: Loader {
                id: slot
                required property string modelData
                required property int index
                readonly property bool level: ["volume", "brightness", "mic-volume", "keyboard"].includes(modelData)
                Layout.fillWidth: true
                Layout.minimumWidth: 0
                Layout.preferredWidth: 1
                Layout.columnSpan: root.span(index)
                sourceComponent: level ? levels : modelData === "appearance" ? appearance : tile
                Component {
                    id: levels
                    NacreControlLevels {
                        width: slot.width
                        kind: slot.modelData
                        monitor: NacreBrightness.getMonitorForScreen(root.screen)
                        onDetailsRequested: (section, pointerOwned) => root.detailsRequested(section, pointerOwned)
                    }
                }
                Component {
                    id: appearance
                    NacreControlAppearance {
                        width: slot.width
                    }
                }
                Component {
                    id: tile
                    NacreControlTile {
                        readonly property var entry: catalog.find(slot.modelData) || {
                            name: "",
                            icon: "",
                            status: "",
                            on: false,
                            available: false
                        }
                        objectName: "control" + (({
                                    network: "Wifi",
                                    bluetooth: "Bluetooth",
                                    battery: "PowerMode",
                                    dnd: "Dnd",
                                    night: "NightLight",
                                    awake: "Awake",
                                    airplane: "Airplane",
                                    vpn: "Vpn",
                                    record: "Record",
                                    mic: "Mic"
                                })[slot.modelData] || "Action-" + slot.modelData)
                        width: slot.width
                        label: entry.name
                        icon: entry.icon
                        detail: entry.status
                        elideDetail: ["network", "bluetooth"].includes(slot.modelData)
                        selected: entry.on
                        available: entry.available
                        navigation: ["network", "bluetooth", "battery", "notifications"].includes(slot.modelData)
                        contextDetails: ["network", "bluetooth", "battery", "night", "dnd", "awake", "airplane", "vpn", "record", "mic", "notifications"].includes(slot.modelData)
                        explanation: slot.modelData === "night" && !entry.available ? NacreControlTools.data.nightLightReason || NacreI18n.tr("controlcenter.details.night-unavailable", "Night light is unavailable on this compositor.") : ""
                        onClicked: {
                            root.pointerActivation = pointerActivation;
                            catalog.toggle(slot.modelData);
                        }
                        onDetailsRequested: root.detailsRequested(slot.modelData === "dnd" ? "notifications" : slot.modelData, pointerActivation)
                    }
                }
            }
        }
    }
    Loader {
        id: noticesLoader
        objectName: "controlNotificationLoader"
        width: parent.width
        active: root.active
        source: Qt.resolvedUrl("NacreControlNotices.qml")
        Connections {
            target: noticesLoader.item
            function onDetailsRequested(section, pointerOwned) {
                root.detailsRequested(section, pointerOwned);
            }
        }
    }
    NacreText {
        width: parent.width
        visible: text !== ""
        text: NacreControlTools.error || NacreControlPreferences.error || ""
        font.pointSize: 9
        color: NacreColours.palette.m3error
        wrapMode: Text.Wrap
    }
}
