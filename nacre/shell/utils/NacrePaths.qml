pragma Singleton
import QtQuick
import Quickshell

QtObject {
    readonly property string home: "file://" + Quickshell.env("HOME")
    // Installed helper paths match the source/package installer. Commands need
    // filesystem paths, while media/config consumers use the URLs below.
    readonly property string share: Quickshell.env("HOME") + "/.local/share/nacre"
    readonly property string tools: share + "/shell/tools"
    readonly property string shellBin: share + "/shell/bin"
    readonly property string palettePython: share + "/palette-runtime/venv/bin/python"
    readonly property string compositorTools: Quickshell.env("NACRE_COMPOSITOR_TOOLS") || Quickshell.env("NACRE_PEARL_TOOLS") || tools
    readonly property string pearlTools: Quickshell.env("NACRE_PEARL_TOOLS") || tools
    readonly property string dashboardTools: Quickshell.env("NACRE_DASHBOARD_TOOLS") || tools
    readonly property string userBin: home.slice(7) + "/.local/bin"
    readonly property string configRoot: fileRoot("XDG_CONFIG_HOME", "/.config")
    readonly property string cacheRoot: fileRoot("XDG_CACHE_HOME", "/.cache")
    readonly property string pictures: home + "/Pictures"
    readonly property string config: configRoot + "/nacre"
    readonly property string state: fileRoot("XDG_STATE_HOME", "/.local/state") + "/nacre"
    readonly property string cache: cacheRoot + "/nacre"
    function fileRoot(variable, fallback): string {
        const value = Quickshell.env(variable);
        return "file://" + (value && value.startsWith("/") ? value : Quickshell.env("HOME") + fallback);
    }
}
