pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Services.UPower

Singleton {
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
            windowCapture: NacreCompositor.capabilities?.windowCapture === true
        };
        for (const key of ["gapsIn", "gapsOut", "borderSize", "rounding", "blur", "shadow", "followMouse", "naturalScroll"])
            result["desktop." + key] = !NacreCompositor.capabilities?.desktopOptions || options.includes(key);
        return result;
    }
}
