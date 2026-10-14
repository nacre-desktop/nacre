pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Services.UPower

Singleton {
    readonly property FileView inputDevices: FileView {
        path: "/proc/bus/input/devices"
        blockLoading: true
        printErrors: false
    }
    readonly property var snapshot: {
        const options = NacreCompositor.capabilities?.desktopOptions || [];
        const result = {
            battery: UPower.displayDevice?.isLaptopBattery === true && UPower.displayDevice?.isPresent === true,
            wifi: NacreNetwork.wifiAvailable,
            bluetooth: NacreBluetooth.available,
            nightLight: NacreControlTools.data.nightLightSupported === true,
            powerProfiles: NacrePowerPolicy.available,
            performanceProfile: PowerProfiles.hasPerformanceProfile,
            ai: NacreProfile.ai,
            brain: NacreProfile.brain,
            packages: NacreMenu.snapshot.capabilities?.packages === true,
            panelDragging: NacreCompositor.capabilities?.nativePanelDrag === true,
            windowCapture: NacreCompositor.capabilities?.windowCapture === true
        };
        result["input.controls"] = ["hyprland", "niri"].includes(NacreCompositor.backendName);
        result["input.touchpadSpeed"] = NacreCompositor.backendName === "niri";
        result.touchpad = /touchpad|trackpad/i.test(inputDevices.text());
        for (const key of ["gapsIn", "gapsOut", "borderSize", "rounding", "blur", "shadow", "followMouse", "naturalScroll"])
            result["desktop." + key] = !NacreCompositor.capabilities?.desktopOptions || options.includes(key);
        return result;
    }
}
