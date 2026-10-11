import QtQuick

Text {
    id: root

    property bool animate: false
    property string animateProp: "scale"
    property real animateFrom: 0
    property real animateTo: 1
    property int animateDuration: NacreTokens.textDuration
    property bool ready: false
    readonly property bool textTransitionRunning: textTween.running

    color: NacreTokens.ink
    font.family: NacreTokens.textFamily
    font.pointSize: NacreTokens.textPointSize
    textFormat: Text.PlainText
    renderType: Text.NativeRendering

    function settleText() {
        if (textTween && textTween.running)
            textTween.complete();
    }
    function transitionText() {
        settleText();
        if (ready && animate && visible && NacreTokens.motionEnabled && animateDuration > 0)
            textTween.restart();
    }

    Component.onCompleted: ready = true
    onTextChanged: transitionText()
    onVisibleChanged: if (!visible)
        settleText()
    onAnimateChanged: if (!animate)
        settleText()

    PropertyAnimation {
        id: textTween
        target: root
        property: root.animateProp
        from: root.animateFrom
        to: root.animateTo
        duration: Math.max(0, root.animateDuration)
        easing.type: Easing.OutCubic
    }
    Connections {
        target: NacreTokens
        function onMotionEnabledChanged() {
            if (!NacreTokens.motionEnabled)
                root.settleText();
        }
    }
}
