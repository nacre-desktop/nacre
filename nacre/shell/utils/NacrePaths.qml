pragma Singleton
import QtQuick
import Quickshell

QtObject {
    readonly property string home: "file://" + Quickshell.env("HOME")
    readonly property string pictures: home + "/Pictures"
    readonly property string config: fileRoot("XDG_CONFIG_HOME", "/.config") + "/nacre"
    readonly property string state: fileRoot("XDG_STATE_HOME", "/.local/state") + "/nacre"
    readonly property string cache: fileRoot("XDG_CACHE_HOME", "/.cache") + "/nacre"
    function fileRoot(variable, fallback): string {
        const value = Quickshell.env(variable);
        return "file://" + (value && value.startsWith("/") ? value : Quickshell.env("HOME") + fallback);
    }
}
