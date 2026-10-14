import QtQuick
import Quickshell.Services.UPower
import qs.widgets
import qs.services
import qs.config

FocusScope {
    id: root
    function prepareLift() {
        revealMotion.complete();
    }
    readonly property bool opening: visibility && revealMotion.running
    property bool restoreImmediately: false
    function restoreAttached() {
        revealMotion.complete();
        syncSize();
    }
    property bool handoffHidden: false
    opacity: handoffHidden ? 0 : 1
    onEnabledChanged: if (enabled)
        handoffHidden = false
    property bool detached: false
    property bool hostActive: true
    required property var screen
    required property bool visibility
    property bool keyboardActive: false
    property string section: "home"
    signal sectionRequested(string section)
    property real presentedWidth: visibility ? contentWidth : 0
    readonly property real contentWidth: Math.max(240, Math.min(detached ? 560 : 424, parent?.width ?? 424))
    readonly property real contentHeight: Math.max(240, Math.min(detached ? 860 : 650, parent?.height ?? 650))
    implicitWidth: Math.min(contentWidth, presentedWidth)
    implicitHeight: contentHeight
    visible: width > 0
    enabled: visibility
    clip: true
    function syncSize() {
        presentedWidth = visibility ? contentWidth : 0;
    }
    function focusExplicit() {
        if (visibility && (keyboardActive || detached))
            forceActiveFocus(Qt.OtherFocusReason);
    }
    onKeyboardActiveChanged: Qt.callLater(focusExplicit)
    onVisibilityChanged: {
        Qt.callLater(syncSize);
        Qt.callLater(focusExplicit);
    }
    onContentWidthChanged: syncSize()
    Component.onCompleted: {
        syncSize();
        Qt.callLater(focusExplicit);
    }
    Behavior on presentedWidth {
        enabled: NacreTokens.motionEnabled && (!root.restoreImmediately || !root.visibility)
        NumberAnimation {
            id: revealMotion
            duration: 280
            easing.type: Easing.OutCubic
        }
    }
    Connections {
        target: NacreTokens
        function onMotionEnabledChanged() {
            if (!NacreTokens.motionEnabled) {
                revealMotion.stop();
                root.syncSize();
            }
        }
    }
    readonly property var detailNames: ({
            home: NacreI18n.tr("controlcenter.control-center.text.controls", "Controls"),
            network: NacreI18n.tr("controlcenter.control-center.label.wi-fi", "Wi-Fi"),
            bluetooth: NacreI18n.tr("controlcenter.control-center.label.bluetooth", "Bluetooth"),
            audio: NacreI18n.tr("popouts.sound-popup.text.sound", "Sound"),
            battery: NacreI18n.tr("controlcenter.details.power-brightness", "Power & brightness"),
            session: NacreI18n.tr("controlcenter.session.title", "Power"),
            night: NacreI18n.tr("controlcenter.control-center.label.night-light", "Night light"),
            dnd: NacreI18n.tr("controlcenter.overview.do-not-disturb", "Do Not Disturb"),
            awake: NacreI18n.tr("controlcenter.overview.stay-awake", "Stay awake"),
            theme: NacreI18n.tr("controlcenter.overview.appearance", "Appearance"),
            airplane: NacreI18n.tr("controlcenter.overview.airplane", "Airplane mode"),
            vpn: NacreI18n.tr("controlcenter.overview.vpn", "VPN"),
            record: NacreI18n.tr("controlcenter.overview.recorder", "Screen recorder"),
            mic: NacreI18n.tr("controlcenter.overview.mic-mute", "Mic mute"),
            edit: NacreI18n.tr("controlcenter.edit.title", "Edit controls")
        })
    function navigate(name) {
        if (!detailNames[name])
            return;
        if (keyboardActive || detached)
            forceActiveFocus(Qt.OtherFocusReason);
        sectionRequested(name);
    }
    Keys.onEscapePressed: event => {
        if (section !== "home") {
            navigate("home");
            event.accepted = true;
        } else
            event.accepted = false;
    }
    Loader {
        id: bodyLoader
        objectName: "controlBodyLoader"
        active: root.visibility || root.presentedWidth > 0
        width: root.contentWidth
        height: root.contentHeight
        onActiveChanged: {
            if (active)
                setSource(Qt.resolvedUrl("NacreControlSurface.qml"), {
                    panel: root
                });
            else
                source = "";
        }
        Component.onCompleted: if (active)
            setSource(Qt.resolvedUrl("NacreControlSurface.qml"), {
                panel: root
            })
    }
    readonly property bool batteryAvailable: !!UPower.displayDevice && UPower.displayDevice.ready && UPower.displayDevice.isPresent !== false && UPower.displayDevice.isLaptopBattery && Number.isFinite(UPower.displayDevice.percentage) && UPower.displayDevice.percentage >= 0 && UPower.displayDevice.percentage <= 1
    readonly property string batteryEstimate: {
        if (!batteryAvailable)
            return "";
        const seconds = UPower.onBattery ? UPower.displayDevice.timeToEmpty : UPower.displayDevice.timeToFull;
        if (!Number.isFinite(seconds) || seconds <= 0)
            return UPower.onBattery ? NacreI18n.tr("battery.remaining-unavailable", "Remaining time unavailable") : NacreI18n.tr("battery.connected-power", "Connected to power");
        const minutes = Math.ceil(seconds / 60);
        return NacreI18n.tr(UPower.onBattery ? "battery.remaining-hours" : "battery.until-full-hours", UPower.onBattery ? "{hours}h {minutes}m remaining" : "{hours}h {minutes}m until full", {
            hours: Math.floor(minutes / 60),
            minutes: minutes % 60
        });
    }
}
