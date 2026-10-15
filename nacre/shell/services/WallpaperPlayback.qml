pragma Singleton
import QtQuick
import qs.utils
import Quickshell
import Quickshell.Services.UPower
import Quickshell.Io

Singleton {
    id: root

    property bool sleeping: false
    property bool locked: false
    readonly property bool batteryPaused: NacreWallpapers.preferences.motionMode === "still" || (NacreWallpapers.preferences.motionMode === "battery" && UPower.onBattery)
    property bool paused: NacreWallpapers.preferences.paused ?? false
    readonly property bool pauseCovered: NacreWallpapers.preferences.pauseCovered ?? true
    property string sessionPath: ""

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
        command: [NacrePaths.helperBinary, "wallpaper-media", "session", "--watch"]

        stdout: SplitParser {
            onRead: line => {
                try {
                    const value = JSON.parse(line);
                    root.locked = value.locked ?? false;
                    root.sessionPath = value.path ?? "";
                    root.sleeping = value.sleeping ?? false;
                } catch (e) {
                    console.warn("WallpaperPlayback: Cannot decode wallpaper session state:", e);
                }
            }
        }
    }
}
