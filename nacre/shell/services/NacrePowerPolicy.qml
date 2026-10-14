pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Services.UPower

Singleton {
    id: root
    readonly property var preferences: NacreSettingsStorage.documents.power || ({})
    property bool serviceReady: false
    readonly property bool installed: NacreControlTools.data.powerProfilesSupported === true
    readonly property bool available: installed && serviceReady
    readonly property string desired: NacreSettingsStorage.present.power === true && preferences.enabled === true && !NacreSettingsStorage.errors?.power ? preferences[UPower.onBattery ? "battery" : "ac"] || "" : ""
    property string error: ""
    function refresh() {
        if (installed && !probe.running)
            probe.running = true;
        else if (!installed)
            serviceReady = false;
    }
    onInstalledChanged: refresh()
    Component.onCompleted: refresh()
    Process {
        id: probe
        // Avoid the CLI env-python shebang selecting the isolated palette venv.
        command: ["/usr/bin/python3", "/usr/bin/powerprofilesctl", "get"]
        onExited: (code, status) => root.serviceReady = status === 0 && code === 0
    }
    function apply() {
        if (!available || !["power-saver", "balanced", "performance"].includes(desired) || desired === "performance" && !PowerProfiles.hasPerformanceProfile)
            return;
        const profiles = {
            "power-saver": PowerProfile.PowerSaver,
            balanced: PowerProfile.Balanced,
            performance: PowerProfile.Performance
        };
        if (PowerProfiles.profile !== profiles[desired])
            PowerProfiles.profile = profiles[desired];
    }
    onDesiredChanged: apply()
    onAvailableChanged: apply()
    Connections {
        target: PowerProfiles
        function onHasPerformanceProfileChanged() {
            root.apply();
        }
    }
    Connections {
        target: UPower
        function onOnBatteryChanged() {
            root.refresh();
        }
    }
}
