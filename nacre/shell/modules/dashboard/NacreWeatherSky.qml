import QtQuick
import qs.widgets
import qs.services

Canvas {
    id: root
    property bool active: false
    property real phase: 0
    readonly property bool day: NacreWeather.reading.isDay !== false
    readonly property int code: NacreWeather.reading.wmo ?? 0
    readonly property color first: NacreTokens.accent
    readonly property color second: NacreTokens.orientSecondary
    readonly property color body: NacreTokens.body
    readonly property bool moving: active && NacreTokens.motionEnabled && !WallpaperPlayback.sleeping && !WallpaperPlayback.locked && (code >= 2 || !day)
    onFirstChanged: requestPaint()
    onSecondChanged: requestPaint()
    onBodyChanged: requestPaint()
    onDayChanged: requestPaint()
    onCodeChanged: requestPaint()
    onWidthChanged: requestPaint()
    onHeightChanged: requestPaint()
    onPhaseChanged: requestPaint()
    Timer {
        interval: 120
        repeat: true
        running: root.moving
        onTriggered: root.phase += .007
    }
    onPaint: {
        const ctx = getContext("2d");
        ctx.reset();
        const gradient = ctx.createLinearGradient(0, 0, width, height);
        gradient.addColorStop(0, Qt.tint(body, Qt.alpha(first, day ? .2 : .08)));
        gradient.addColorStop(1, Qt.tint(body, Qt.alpha(second, day ? .24 : .12)));
        ctx.fillStyle = gradient;
        ctx.fillRect(0, 0, width, height);
        if (!day) {
            for (let i = 0; i < 35; i++) {
                ctx.fillStyle = Qt.alpha("#fffdf6", .2 + .2 * Math.sin(phase + i));
                ctx.beginPath();
                ctx.arc((i * 137.7) % width, (i * 31.1) % (height * .7), 1, 0, Math.PI * 2);
                ctx.fill();
            }
        }
        ctx.fillStyle = Qt.alpha(first, .32);
        ctx.beginPath();
        ctx.arc(width * .82, height * .28, day ? 30 : 24, 0, Math.PI * 2);
        ctx.fill();
        if (!day) {
            ctx.fillStyle = body;
            ctx.beginPath();
            ctx.arc(width * .82 + 10, height * .28 - 5, 23, 0, Math.PI * 2);
            ctx.fill();
        }
        if (code >= 2) {
            for (let i = 0; i < 5; i++) {
                const x = ((i * width / 4 + phase * 25) % (width + 180)) - 90, y = height * .28 + i % 2 * 20;
                ctx.fillStyle = Qt.alpha(second, code >= 45 ? .09 : .12);
                ctx.save();
                ctx.translate(x, y);
                ctx.scale(120, 28);
                ctx.beginPath();
                ctx.arc(0, 0, 1, 0, Math.PI * 2);
                ctx.fill();
                ctx.restore();
            }
        }
        if (code >= 51) {
            const snow = code >= 71 && code <= 77 || code >= 85 && code <= 86;
            ctx.strokeStyle = Qt.alpha(first, .3);
            ctx.lineWidth = 1.2;
            for (let i = 0; i < 24; i++) {
                const x = i * width / 24, y = (i * 19 + phase * 110) % height;
                ctx.beginPath();
                ctx.moveTo(x, y);
                ctx.lineTo(x + (snow ? 1 : -4), y + (snow ? 2 : 11));
                ctx.stroke();
            }
        }
    }
}
