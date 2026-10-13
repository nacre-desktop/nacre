import QtQuick
import qs.widgets
import qs.services

Item {
    id: root
    property bool active: false
    readonly property bool day: NacreWeather.reading.isDay !== false
    readonly property int code: NacreWeather.reading.wmo ?? 0
    readonly property int hour: Number((NacreWeather.reading.localTime || "").slice(11, 13))
    readonly property bool twilight: day && (hour < 8 || hour >= 18)
    readonly property bool moving: active && NacreTokens.motionEnabled && !WallpaperPlayback.sleeping && !WallpaperPlayback.locked
    Rectangle {
        anchors.fill: parent
        gradient: Gradient {
            GradientStop {
                position: 0
                color: Qt.tint(NacreTokens.body, Qt.alpha(root.twilight ? NacreColours.palette.m3tertiary : NacreTokens.accent, root.day ? .24 : .14))
            }
            GradientStop {
                position: .6
                color: Qt.tint(NacreTokens.body, Qt.alpha(NacreTokens.orientSecondary, root.code >= 45 ? .14 : .24))
            }
            GradientStop {
                position: 1
                color: NacreTokens.raised
            }
        }
    }
    Canvas {
        id: atmosphere
        width: parent.width + 36
        height: parent.height
        property color first: NacreTokens.accent
        property color second: NacreTokens.orientSecondary
        property bool day: root.day
        property int condition: root.code
        onFirstChanged: requestPaint()
        onSecondChanged: requestPaint()
        onDayChanged: requestPaint()
        onConditionChanged: requestPaint()
        onWidthChanged: requestPaint()
        onHeightChanged: requestPaint()
        SequentialAnimation on x {
            running: root.moving
            loops: Animation.Infinite
            NumberAnimation {
                from: -18
                to: 0
                duration: 60000
                easing.type: Easing.InOutSine
            }
            NumberAnimation {
                from: 0
                to: -18
                duration: 60000
                easing.type: Easing.InOutSine
            }
        }
        onPaint: {
            const ctx = getContext("2d");
            ctx.reset();
            const sx = width * .8, sy = height * .2;
            const glow = ctx.createRadialGradient(sx, sy, 8, sx, sy, 100);
            glow.addColorStop(0, Qt.alpha(first, .2));
            glow.addColorStop(1, Qt.alpha(first, 0));
            ctx.fillStyle = glow;
            ctx.fillRect(0, 0, width, height);
            ctx.fillStyle = Qt.alpha(first, .3);
            ctx.beginPath();
            ctx.arc(sx, sy, day ? 25 : 18, 0, Math.PI * 2);
            ctx.fill();
            if (!day) {
                ctx.fillStyle = Qt.tint(NacreTokens.body, Qt.alpha(first, .14));
                ctx.beginPath();
                ctx.arc(sx + 8, sy - 5, 17, 0, Math.PI * 2);
                ctx.fill();
                for (let i = 0; i < 18; i++) {
                    ctx.fillStyle = Qt.alpha(NacreTokens.ink, .18);
                    ctx.fillRect((i * 137) % width, (i * 43) % (height * .65), 1, 1);
                }
            }
            if (condition >= 2) {
                for (let i = 0; i < 3; i++) {
                    const x = width * (.35 + i * .22), y = height * (.18 + (i % 2) * .08), size = 36 + i * 10;
                    ctx.save();
                    ctx.translate(x, y);
                    ctx.scale(size, size);
                    ctx.fillStyle = Qt.alpha(second, condition >= 45 ? .1 : .08);
                    ctx.beginPath();
                    ctx.moveTo(-1.8, .35);
                    ctx.bezierCurveTo(-2.2, -.1, -1.6, -.45, -1.1, -.3);
                    ctx.bezierCurveTo(-1.1, -1.2, .1, -1.3, .55, -.6);
                    ctx.bezierCurveTo(1.1, -.85, 1.8, -.5, 1.7, .1);
                    ctx.bezierCurveTo(2.5, .15, 2.2, .85, 1.4, .75);
                    ctx.bezierCurveTo(.4, .85, -.9, .7, -1.8, .35);
                    ctx.closePath();
                    ctx.fill();
                    ctx.restore();
                }
            }
        }
    }
}
