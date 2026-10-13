import QtQuick
import qs.widgets

Canvas {
    id: root
    property var values: []
    property var secondaryValues: []
    property real maximum: 1
    property color first: NacreTokens.accent
    property color second: NacreTokens.orientSecondary
    implicitHeight: 46
    onValuesChanged: requestPaint()
    onSecondaryValuesChanged: requestPaint()
    onMaximumChanged: requestPaint()
    onFirstChanged: requestPaint()
    onSecondChanged: requestPaint()
    onWidthChanged: requestPaint()
    onHeightChanged: requestPaint()
    onPaint: {
        const ctx = getContext("2d");
        ctx.reset();
        const draw = (data, color) => {
            if (!data.length)
                return;
            ctx.strokeStyle = color;
            ctx.lineWidth = 2;
            ctx.lineJoin = "round";
            ctx.beginPath();
            data.forEach((value, i) => {
                const x = 2 + i * (width - 4) / Math.max(1, data.length - 1);
                const y = height - 3 - Math.max(0, Math.min(1, value / Math.max(1, root.maximum))) * (height - 6);
                if (i)
                    ctx.lineTo(x, y);
                else
                    ctx.moveTo(x, y);
            });
            ctx.stroke();
        };
        draw(root.values, root.first);
        draw(root.secondaryValues, root.second);
    }
}
