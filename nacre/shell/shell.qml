import QtQuick
import Quickshell
import qs.services
import qs.modules
import qs.modules.drawers
import qs.modules.background
import qs.modules.extras
import qs.modules.settings
import qs.modules.colors
import qs.modules.pearls
import qs.modules.welcome

// While `nacre ui-shell bead` is active this process hosts only the windows below: no bar, panels,
// wallpaper, shortcuts, UI-provider role, lock widgets or notification server.
ShellRoot {
    readonly property bool windowsOnly: Quickshell.env("NACRE_QML_WINDOWS_ONLY") === "1"
    property var helperService: NacreHelpers
    property var controlTools: NacreControlTools
    property var batteryAlerts: NacreBatteryAlerts
    property var powerPolicy: NacrePowerPolicy
    property var lockWidgets: windowsOnly ? null : LockWidgets
    property var displayRecovery: DisplayRecovery
    LazyLoader {
        active: !windowsOnly
        component: Scope {
            NacreBackground {}
            NacreDesktop {}
            NacreDockGuide {}
            NacreShellShortcuts {}
            NacreShellIpc {}
        }
    }
    NacreSettingsHost {}
    NacreColorsHost {}
    NacreWelcomeHost {}
    Component.onCompleted: ChatWindowTitle.scan()
}
