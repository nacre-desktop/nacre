pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Services.UPower
import Quickshell.Io

Singleton {
    id: root

    property bool sleeping: false
    property bool locked: false
    property string awaiting: ""
    readonly property bool batteryPaused: NacreWallpapers.preferences.motionMode === "still" || (NacreWallpapers.preferences.motionMode === "battery" && UPower.onBattery)
    property bool paused: NacreWallpapers.preferences.paused ?? false
    readonly property bool pauseCovered: NacreWallpapers.preferences.pauseCovered ?? true
    property string sessionPath: ""
    property bool sessionSignal: false

    IpcHandler {
        target: "wallpaperPlayback"
        function state(): string {
            return JSON.stringify({
                motionMode: NacreWallpapers.preferences.motionMode,
                onBattery: UPower.onBattery,
                batteryPaused: root.batteryPaused,
                locked: root.locked,
                sleeping: root.sleeping
            });
        }
    }
    Process {
        running: true
        command: ["python3", Quickshell.env("HOME") + "/.local/share/nacre/shell/tools/wallpaper-media.py", "session"]

        stdout: SplitParser {
            splitMarker: ""
            onRead: line => {
                try {
                    const value = JSON.parse(line);
                    root.locked = value.locked ?? false;
                    root.sessionPath = value.path ?? "";
                } catch (e) {}
            }
        }
    }

    Process {
        running: true
        command: ["dbus-monitor", "--system", "type='signal',interface='org.freedesktop.login1.Manager',member='PrepareForSleep'", "type='signal',interface='org.freedesktop.DBus.Properties',member='PropertiesChanged',arg0='org.freedesktop.login1.Session'"]

        stdout: SplitParser {
            onRead: line => {
                if (line.includes("member=PrepareForSleep")) {
                    root.awaiting = "sleep";
                } else if (line.includes("member=PropertiesChanged")) {
                    root.awaiting = "";
                    root.sessionSignal = !!root.sessionPath && line.includes("path=" + root.sessionPath + ";");
                } else if (line.trim() === 'string "LockedHint"') {
                    root.awaiting = root.sessionSignal ? "lock" : "";
                } else if (line.trim().startsWith("boolean ")) {
                    const value = line.trim().endsWith("true");
                    if (root.awaiting === "sleep")
                        root.sleeping = value;
                    else if (root.awaiting === "lock")
                        root.locked = value;
                    root.awaiting = "";
                }
            }
        }
    }
}
