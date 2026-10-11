import Quickshell
import Quickshell.Wayland

PanelWindow {
    property string name: "panel"
    color: "transparent"
    exclusionMode: ExclusionMode.Auto
    exclusiveZone: 0
    WlrLayershell.namespace: "nacre-" + name
    WlrLayershell.layer: WlrLayer.Top
    WlrLayershell.keyboardFocus: WlrKeyboardFocus.None
}
