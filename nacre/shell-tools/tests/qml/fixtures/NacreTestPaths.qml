import QtQuick
import Quickshell

QtObject {
    property string home: "file:///fixture"
    property string share: home.slice(7) + "/.local/share/nacre"
    property string tools: share + "/shell/tools"
    property string helperBinary: tools + "/nacre-helper"
    property string shellBin: share + "/shell/bin"
    property string compositorTools: Quickshell.env("NACRE_COMPOSITOR_TOOLS") || Quickshell.env("NACRE_PEARL_TOOLS") || tools
    property string pearlTools: Quickshell.env("NACRE_PEARL_TOOLS") || tools
    property string dashboardTools: Quickshell.env("NACRE_DASHBOARD_TOOLS") || tools
    property string userBin: home.slice(7) + "/.local/bin"
    property string configRoot: fileRoot("XDG_CONFIG_HOME", "/.config")
    property string cacheRoot: fileRoot("XDG_CACHE_HOME", "/.cache")
    property string pictures: home + "/Pictures"
    property string config: configRoot + "/nacre"
    property string state: fileRoot("XDG_STATE_HOME", "/.local/state") + "/nacre"
    property string cache: cacheRoot + "/nacre"
    function fileRoot(variable, fallback) {
        const value = Quickshell.env(variable);
        return "file://" + (value && value.startsWith("/") ? value : home.slice(7) + fallback);
    }
}
