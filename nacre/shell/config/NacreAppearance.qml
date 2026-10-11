pragma Singleton
import QtQuick
import qs.widgets

QtObject {
    property QtObject rounding: QtObject {
        property int small: 12
        property int normal: 17
        property int large: 25
        property int full: 1000
    }
    property QtObject spacing: QtObject {
        property int small: 7
        property int smaller: 10
        property int normal: 12
        property int larger: 15
        property int large: 20
    }
    property QtObject padding: QtObject {
        property int small: 5
        property int smaller: 7
        property int normal: 10
        property int larger: 12
        property int large: 15
    }
    property QtObject font: QtObject {
        property QtObject family: QtObject {
            property string sans: NacreTokens.textFamily
            property string mono: NacreTokens.monoFamily
            property string material: "Material Symbols Rounded"
        }
        property QtObject size: QtObject {
            property int small: 11
            property int smaller: 12
            property int normal: 13
            property int larger: 15
            property int large: 18
            property int extraLarge: 28
        }
    }
    property QtObject anim: QtObject {
        property QtObject durations: QtObject {
            readonly property int small: NacreTokens.motionEnabled ? 200 : 0
            readonly property int normal: NacreTokens.motionEnabled ? 400 : 0
            readonly property int large: NacreTokens.motionEnabled ? 600 : 0
            readonly property int extraLarge: NacreTokens.motionEnabled ? 1000 : 0
            readonly property int expressiveFastSpatial: NacreTokens.motionEnabled ? 350 : 0
            readonly property int expressiveDefaultSpatial: NacreTokens.motionEnabled ? 500 : 0
            readonly property int expressiveEffects: NacreTokens.motionEnabled ? 200 : 0
        }
        property QtObject curves: QtObject {
            // Compatibility property names; all curves authored as ordinary bounded easing.
            readonly property var standard: [0.2, 0.7, 0.3, 1, 1, 1]
            readonly property var emphasized: [0.18, 0.8, 0.28, 1, 1, 1]
            readonly property var emphasizedAccel: [0.5, 0, 0.8, 0.4, 1, 1]
            readonly property var emphasizedDecel: [0.12, 0.8, 0.25, 1, 1, 1]
            readonly property var standardAccel: [0.4, 0, 0.8, 0.4, 1, 1]
            readonly property var standardDecel: [0.15, 0.7, 0.3, 1, 1, 1]
            readonly property var expressiveFastSpatial: [0.15, 0.85, 0.3, 1, 1, 1]
            readonly property var expressiveDefaultSpatial: [0.2, 0.8, 0.35, 1, 1, 1]
            readonly property var expressiveEffects: [0.2, 0.7, 0.3, 1, 1, 1]
        }
    }
}
