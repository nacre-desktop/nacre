pragma Singleton
import Quickshell

Singleton {
    readonly property var all: NacreApps.all
    readonly property var list: NacreApps.list
    readonly property var preppedApps: NacreApps.preppedApps
    function fuzzyQuery(search) {
        return NacreApps.fuzzyQuery(search);
    }
    function launch(entry) {
        return NacreApps.launch(entry);
    }
}
