import QtQuick

// Providers are injected so the preview cannot query the live desktop. The host
// must publish facts; a missing or unknown capability always evaluates false.
QtObject {
    property var compositor: ({})
    property bool battery: false
    property bool wifi: false
    property bool bluetooth: false
    property bool nightLight: false
    property bool ai: false
    property bool brain: false
    property bool packages: false
    property bool paru: false
    property bool yay: false
    property bool flatpak: false
    readonly property var snapshot: Object.assign({}, compositor, {
        controlDetected: true,
        controlAvailable: true,
        battery: battery,
        wifi: wifi,
        bluetooth: bluetooth,
        nightLight: nightLight,
        powerProfiles: true,
        performanceProfile: true,
        ai: ai,
        brain: brain,
        packages: packages,
        paru: paru,
        yay: yay,
        flatpak: flatpak
    })
}
