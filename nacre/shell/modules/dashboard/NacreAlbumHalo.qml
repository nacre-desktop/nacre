import QtQuick
import qs.widgets
import qs.services

Canvas {
    id: root
    property var samples: []
    property real coverSize: 120
    property bool active: false
    readonly property color primary: NacreTokens.accent
    readonly property color secondary: NacreTokens.orientSecondary
    readonly property color tertiary: NacreColours.palette.m3tertiary
    readonly property bool audible: active && samples.some(value => Number.isFinite(value) && value > .005)
    visible: audible
    onSamplesChanged: requestPaint()
    onActiveChanged: requestPaint()
    onPrimaryChanged: requestPaint()
    onSecondaryChanged: requestPaint()
    onTertiaryChanged: requestPaint()
    onWidthChanged: requestPaint()
    onHeightChanged: requestPaint()
    onPaint: {
        const ctx = getContext("2d");
        ctx.reset();
        if (!audible)
            return;
        const cx = width / 2, cy = height / 2, radius = coverSize / 2 + 9;
        ctx.lineWidth = 2.5;
        ctx.lineCap = "round";
        for (let i = 0; i < 32; i++) {
            const angle = -Math.PI / 2 + i / 32 * Math.PI * 2;
            const raw = samples[i], level = Number.isFinite(raw) ? Math.max(0, Math.min(1, raw)) : 0;
            const x = cx + radius * Math.cos(angle), y = cy + radius * Math.sin(angle);
            const end = radius + 2 + level * 14;
            const ex = cx + end * Math.cos(angle), ey = cy + end * Math.sin(angle);
            const gradient = ctx.createLinearGradient(x, y, ex, ey);
            gradient.addColorStop(0, String(primary));
            gradient.addColorStop(.5, String(secondary));
            gradient.addColorStop(1, String(tertiary));
            ctx.strokeStyle = gradient;
            ctx.beginPath();
            ctx.moveTo(x, y);
            ctx.lineTo(ex, ey);
            ctx.stroke();
        }
    }
}
