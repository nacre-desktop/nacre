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
    property string applied: ""
    readonly property Process profileWriter: Process {
        property string requested: ""
        command: ["/usr/bin/python3", "/usr/bin/powerprofilesctl", "set", requested]
        stderr: StdioCollector {
            onStreamFinished: if (text.trim())
                root.error = text.trim()
        }
        onExited: code => {
            if (code === 0)
                root.applied = requested;
            else if (!root.error)
                root.error = NacreI18n.tr("settings.registry.power.profiles.failed", "The power profile could not be applied.");
            if (requested !== root.desired)
                Qt.callLater(root.apply);
        }
    }
    function apply() {
        if (!desired)
            applied = "";
        // The daemon applies profiles when it owns the always-on behaviours (N0039).
        if (!available || NacreDaemon.behavioursHeld || profileWriter.running || desired === applied || !["power-saver", "balanced", "performance"].includes(desired) || desired === "performance" && !PowerProfiles.hasPerformanceProfile)
            return;
        // powerprofilesctl reports D-Bus failures; the QML property setter has no acknowledgement.
        error = "";
        profileWriter.requested = desired;
        profileWriter.running = true;
    }
    onDesiredChanged: apply()
    onAvailableChanged: apply()
    Connections {
        target: NacreDaemon
        function onBehavioursHeldChanged() {
            root.apply();
        }
    }
    Connections {
        target: PowerProfiles
        function onHasPerformanceProfileChanged() {
            root.apply();
        }
    }
    Connections {
        target: UPower
        function onOnBatteryChanged() {
            root.applied = "";
            root.refresh();
            Qt.callLater(root.apply);
        }
    }
}
