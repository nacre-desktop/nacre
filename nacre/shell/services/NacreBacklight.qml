import QtQuick

NacreLightChannel {
    id: root
    required property var owner
    required property var modelData
    descriptor: owner.forScreen(modelData)
    present: owner.screenPresent(modelData)
    readonly property string busNum: isDdc ? String(descriptor.bus) : ""
}
