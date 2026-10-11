pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Services.UPower

Singleton {
    id: root

    readonly property string presentation: JSON.stringify(snapshot())
    property bool dirty: true

    function snapshot() {
        const p = NacrePlayers.active;
        return {
            "colors": NacrePresentation.active.colours,
            "wallpaper": NacrePresentation.active.poster ?? NacreWallpapers.poster,
            "system": {
                "cpu": NacreSystemUsage.cpuPerc,
                "memory": NacreSystemUsage.memPerc,
                "storage": NacreSystemUsage.storagePerc,
                "temperature": Number.isFinite(NacreSystemUsage.cpuTemp) ? NacreSystemUsage.cpuTemp : null
            },
            "greetingHour": NacreTime.hours,
            "preferences": {
                "lockMedia": DesktopSettings.data.lockMedia,
                "lockWeather": DesktopSettings.data.lockWeather,
                "lockNotifications": DesktopSettings.data.lockNotifications,
                "lockNotificationContents": DesktopSettings.data.lockNotificationContents
            },
            "weather": {
                "location": NacreWeather.location,
                "icon": NacreWeather.icon,
                "detail": NacreWeather.detail,
                "range": NacreWeather.range,
                "description": NacreWeather.description,
                "temperature": NacreWeather.displayTemperature,
                "stale": NacreWeather.stale,
                "error": NacreWeather.error
            },
            "media": p ? {
                "title": p.trackTitle,
                "artist": p.trackArtist,
                "album": p.trackAlbum,
                "art": p.trackArtUrl,
                "playing": p.isPlaying,
                "canToggle": p.canTogglePlaying
            } : null,
            "notifications": NacreNotifs.retained.slice(-64).reverse().map(n => {
                return ({
                        "app": n.appName,
                        "time": n.time.toISOString(),
                        "icon": n.appIcon,
                        "summary": DesktopSettings.data.lockNotificationContents ? n.summary : "",
                        "body": DesktopSettings.data.lockNotificationContents ? n.body : ""
                    });
            }),
            "count": NacreNotifs.retained.length,
            "battery": UPower.displayDevice.ready && UPower.displayDevice.isLaptopBattery && Number.isFinite(UPower.displayDevice.percentage) && UPower.displayDevice.percentage >= 0 && UPower.displayDevice.percentage <= 1 ? Math.round(UPower.displayDevice.percentage * 100) : null
        };
    }

    onPresentationChanged: {
        dirty = true;
        settle.restart();
    }
    Component.onCompleted: settle.restart()

    Timer {
        id: settle

        interval: 100
        onTriggered: {
            if (!writer.running) {
                root.dirty = false;
                writer.running = true;
            }
        }
    }

    Process {
        id: writer

        stdinEnabled: true
        command: ["python3", Quickshell.env("HOME") + "/.local/share/nacre/shell/tools/lock-prepare.py"]
        onStarted: write(root.presentation + "\n")
        onExited: {
            if (root.dirty)
                settle.restart();
        }
    }

    IpcHandler {
        function state(): string {
            return root.presentation;
        }

        target: "lockWidgets"
    }
}
