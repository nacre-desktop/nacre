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
    readonly property color light: root.twilight ? NacreColours.palette.m3tertiary : NacreTokens.accent
    Rectangle {
        anchors.fill: parent
        gradient: Gradient {
            GradientStop {
                position: 0
                color: Qt.tint(NacreTokens.body, Qt.alpha(NacreTokens.orientSecondary, root.day ? .38 : .24))
            }
            GradientStop {
                position: .56
                color: Qt.tint(NacreTokens.body, Qt.alpha(root.light, root.twilight ? .32 : .16))
            }
            GradientStop {
                position: 1
                color: NacreTokens.raised
            }
        }
    }
    Canvas {
        id: landscape
        anchors.fill: parent
        property color light: root.light
        property color ink: NacreTokens.ink
        property color body: NacreTokens.body
        property color second: NacreTokens.orientSecondary
        property int condition: root.code
        property bool day: root.day
        onLightChanged: requestPaint()
        onInkChanged: requestPaint()
        onBodyChanged: requestPaint()
        onSecondChanged: requestPaint()
        onConditionChanged: requestPaint()
        onDayChanged: requestPaint()
        onWidthChanged: requestPaint()
        onHeightChanged: requestPaint()
        onPaint: {
            const ctx = getContext("2d");
            ctx.reset();
            const sx = width * .8, sy = height * .22;
            const light = ctx.createRadialGradient(sx, sy, 0, sx, sy, width * .48);
            light.addColorStop(0, Qt.alpha(root.light, day ? .32 : .15));
            light.addColorStop(.25, Qt.alpha(root.light, .12));
            light.addColorStop(1, Qt.alpha(root.light, 0));
            ctx.fillStyle = light;
            ctx.fillRect(0, 0, width, height);
            if (!day) {
                for (let i = 0; i < 42; i++) {
                    ctx.fillStyle = Qt.alpha(ink, i % 4 === 0 ? .28 : .12);
                    ctx.beginPath();
                    ctx.arc((i * 137 + 39) % width, (i * 47 + 17) % (height * .58), i % 4 === 0 ? .9 : .5, 0, Math.PI * 2);
                    ctx.fill();
                }
            }
            // A distant shoreline gives depth without placing another icon behind the sun arc.
            for (let i = 0; i < 3; i++) {
                const y = height * (.56 + i * .09);
                ctx.fillStyle = Qt.alpha(body, .14 + i * .12);
                ctx.beginPath();
                ctx.moveTo(0, height);
                ctx.lineTo(0, y + height * .13);
                ctx.bezierCurveTo(width * .12, y + 24, width * .21, y - 18, width * .36, y + 10);
                ctx.bezierCurveTo(width * .5, y + 42, width * .56, y - 35, width * .72, y - 12);
                ctx.bezierCurveTo(width * .86, y - 30, width * .95, y + 14, width, y);
                ctx.lineTo(width, height);
                ctx.closePath();
                ctx.fill();
            }
            const mist = ctx.createLinearGradient(0, height * .55, 0, height);
            mist.addColorStop(0, Qt.alpha(second, 0));
            mist.addColorStop(.35, Qt.alpha(second, .08));
            mist.addColorStop(1, Qt.alpha(body, .35));
            ctx.fillStyle = mist;
            ctx.fillRect(0, height * .55, width, height * .45);
        }
    }
    Canvas {
        id: atmosphere
        width: parent.width + 36
        height: parent.height
        property color cloud: NacreTokens.ink
        property int condition: root.code
        property bool day: root.day
        onCloudChanged: requestPaint()
        onConditionChanged: requestPaint()
        onDayChanged: requestPaint()
        onWidthChanged: requestPaint()
        onHeightChanged: requestPaint()
        SequentialAnimation on x {
            running: root.moving && root.code >= 2
            loops: Animation.Infinite
            NumberAnimation {
                from: -18
                to: 0
                duration: 90000
                easing.type: Easing.InOutSine
            }
            NumberAnimation {
                from: 0
                to: -18
                duration: 90000
                easing.type: Easing.InOutSine
            }
        }
        onPaint: {
            const ctx = getContext("2d");
            ctx.reset();
            if (condition < 2)
                return;
            // Soft density fields have no hard oval or cartoon cloud outline.
            for (let i = 0; i < 9; i++) {
                const x = width * (.34 + (i % 5) * .16), y = height * (.1 + (i % 3) * .08);
                ctx.save();
                ctx.translate(x, y);
                ctx.scale(1, .45);
                const r = width * (.12 + (i % 3) * .035), haze = ctx.createRadialGradient(0, 0, 0, 0, 0, r);
                haze.addColorStop(0, Qt.alpha(cloud, day ? .085 : .045));
                haze.addColorStop(.4, Qt.alpha(cloud, day ? .05 : .025));
                haze.addColorStop(1, Qt.alpha(cloud, 0));
                ctx.fillStyle = haze;
                ctx.fillRect(-r, -r, r * 2, r * 2);
                ctx.restore();
            }
        }
    }
    Rectangle {
        anchors.fill: parent
        gradient: Gradient {
            orientation: Gradient.Horizontal
            GradientStop {
                position: 0
                color: Qt.alpha(NacreTokens.raised, .25)
            }
            GradientStop {
                position: .5
                color: Qt.alpha(NacreTokens.raised, 0)
            }
        }
    }
}
