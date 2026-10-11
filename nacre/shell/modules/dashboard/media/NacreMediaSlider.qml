import QtQuick
import QtQuick.Controls.Basic
import qs.widgets

Slider {
    id: root
    property real progress: 0
    property string label: ""
    property color accent: NacreTokens.accent
    property color second: NacreTokens.orientSecondary
    property color track: NacreTokens.raised
    property real lastUserValue: 0
    property bool dirty: false
    signal dragBegan
    signal dragEnded(real fraction)
    signal keyboardRequested(real fraction)
    from: 0
    to: 1
    value: progress
    live: true
    implicitHeight: 34
    implicitWidth: 260
    leftPadding: 8
    rightPadding: 8
    Accessible.name: label
    onPressedChanged: {
        if (pressed) {
            dirty = false;
            lastUserValue = value;
            dragBegan();
        } else if (dirty) {
            dirty = false;
            dragEnded(lastUserValue);
        }
    }
    onMoved: {
        if (pressed) {
            lastUserValue = value;
            dirty = true;
        } else
            keyboardRequested(value);
    }
    background: NacreSurface {
        x: root.leftPadding
        y: root.topPadding + (root.availableHeight - height) / 2
        width: root.availableWidth
        height: 6
        radius: 3
        color: root.track
        NacreSurface {
            width: root.position * parent.width
            height: parent.height
            radius: 3
            gradient: Gradient {
                orientation: Gradient.Horizontal
                GradientStop {
                    position: 0
                    color: root.accent
                }
                GradientStop {
                    position: 1
                    color: root.second
                }
            }
            opacity: root.enabled ? 1 : .4
        }
    }
    handle: NacreSurface {
        x: root.leftPadding + root.visualPosition * (root.availableWidth - width)
        y: root.topPadding + (root.availableHeight - height) / 2
        width: 16
        height: 16
        radius: 8
        color: "#f2efee"
        border.color: root.accent
        opacity: root.enabled ? 1 : .4
        border.width: root.visualFocus ? 2 : 1
    }
}
