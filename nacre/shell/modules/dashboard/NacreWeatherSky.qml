import QtQuick
import qs.widgets
import qs.services

Item {
    id: root
    property bool active: false
    readonly property bool day: NacreWeather.reading.isDay !== false
    readonly property int code: NacreWeather.reading.wmo ?? -1
    readonly property bool photoEnabled: NacreDashboardState.weatherPhotos === true
    readonly property bool photoReady: cityPhoto.status === Image.Ready && photoEnabled
    readonly property bool moving: active && NacreTokens.motionEnabled && !WallpaperPlayback.sleeping && !WallpaperPlayback.locked
    Image {
        id: cityPhoto
        objectName: "weatherCityPhoto"
        anchors.fill: parent
        source: root.photoEnabled ? NacreWeatherPhoto.image : ""
        fillMode: Image.PreserveAspectCrop
        sourceSize.width: 960
        sourceSize.height: 600
        // A bounded, local thumbnail is decoded before the first painted frame.
        // Qt's image cache also avoids decoding it again on every tab visit.
        asynchronous: false
        cache: true
        retainWhileLoading: true
    }
    Rectangle {
        anchors.fill: parent
        gradient: Gradient {
            GradientStop {
                position: 0
                color: Qt.alpha(NacreTokens.body, root.photoReady ? .6 : .16)
            }
            GradientStop {
                position: .58
                color: Qt.alpha(NacreTokens.body, root.photoReady ? .7 : .28)
            }
            GradientStop {
                position: 1
                color: Qt.alpha(NacreTokens.raised, .94)
            }
        }
    }
    Rectangle {
        anchors.fill: parent
        gradient: Gradient {
            orientation: Gradient.Horizontal
            GradientStop {
                position: 0
                color: Qt.alpha(NacreTokens.body, .65)
            }
            GradientStop {
                position: .52
                color: Qt.alpha(NacreTokens.body, .35)
            }
            GradientStop {
                position: 1
                color: Qt.alpha(NacreTokens.body, .1)
            }
        }
    }
    Canvas {
        id: landscape
        objectName: "weatherIllustration"
        width: parent.width + 28
        height: parent.height
        visible: !root.photoReady
        z: -1
        property color body: NacreTokens.body
        property color ink: NacreTokens.ink
        property color accent: NacreTokens.accent
        property color secondary: NacreTokens.orientSecondary
        property int condition: root.code
        property bool daylight: root.day
        readonly property string scene: condition < 0 ? "unknown" : condition <= 1 ? "clear" : condition <= 3 ? "clouds" : condition === 45 || condition === 48 ? "mist" : condition >= 71 && condition <= 77 || condition === 85 || condition === 86 ? "snow" : condition >= 95 ? "storm" : "rain"
        onBodyChanged: requestPaint()
        onInkChanged: requestPaint()
        onAccentChanged: requestPaint()
        onSecondaryChanged: requestPaint()
        onConditionChanged: requestPaint()
        onDaylightChanged: requestPaint()
        onWidthChanged: requestPaint()
        onHeightChanged: requestPaint()
        onAvailableChanged: if (available)
            requestPaint()
        SequentialAnimation on x {
            running: root.moving && !root.photoReady && landscape.scene !== "clear"
            loops: Animation.Infinite
            NumberAnimation {
                from: -14
                to: 0
                duration: 90000
                easing.type: Easing.InOutSine
            }
            NumberAnimation {
                from: 0
                to: -14
                duration: 90000
                easing.type: Easing.InOutSine
            }
        }
        onPaint: {
            const ctx = getContext("2d");
            ctx.reset();
            const sky = ctx.createLinearGradient(0, 0, 0, height);
            sky.addColorStop(0, Qt.tint(body, Qt.alpha(secondary, daylight ? .35 : .12)));
            sky.addColorStop(.65, Qt.tint(body, Qt.alpha(accent, daylight ? .15 : .04)));
            sky.addColorStop(1, body);
            ctx.fillStyle = sky;
            ctx.fillRect(0, 0, width, height);
            if (scene === "clear" || condition === 2) {
                const sx = width * .77, sy = height * .23, radius = Math.min(38, height * .12);
                const glow = ctx.createRadialGradient(sx, sy, radius * .5, sx, sy, radius * 3.4);
                glow.addColorStop(0, Qt.alpha(daylight ? accent : ink, .2));
                glow.addColorStop(1, Qt.alpha(accent, 0));
                ctx.fillStyle = glow;
                ctx.fillRect(sx - radius * 4, sy - radius * 4, radius * 8, radius * 8);
                ctx.fillStyle = Qt.alpha(daylight ? accent : ink, .85);
                ctx.beginPath();
                ctx.arc(sx, sy, radius, 0, Math.PI * 2);
                ctx.fill();
                if (!daylight) {
                    ctx.fillStyle = Qt.tint(body, Qt.alpha(secondary, .12));
                    ctx.beginPath();
                    ctx.arc(sx + radius * .48, sy - radius * .28, radius * .9, 0, Math.PI * 2);
                    ctx.fill();
                }
            }
            if (scene !== "clear") {
                const cloud = (cx, cy, scale) => {
                    ctx.save();
                    ctx.translate(cx, cy);
                    ctx.scale(scale, scale);
                    const shade = ctx.createLinearGradient(0, -55, 0, 30);
                    shade.addColorStop(0, Qt.tint(body, Qt.alpha(ink, daylight ? .3 : .14)));
                    shade.addColorStop(1, Qt.tint(body, Qt.alpha(secondary, .09)));
                    ctx.fillStyle = shade;
                    ctx.beginPath();
                    ctx.moveTo(-95, 18);
                    ctx.bezierCurveTo(-132, 14, -120, -30, -87, -30);
                    ctx.bezierCurveTo(-86, -71, -27, -78, -13, -44);
                    ctx.bezierCurveTo(15, -72, 59, -54, 63, -25);
                    ctx.bezierCurveTo(112, -39, 140, 23, 90, 27);
                    ctx.bezierCurveTo(27, 38, -51, 31, -95, 18);
                    ctx.closePath();
                    ctx.fill();
                    ctx.restore();
                };
                cloud(width * .91, height * .17, .7);
                cloud(width * .56, height * .21, .75);
                cloud(width * .8, height * .37, 1.1);
            }
            if (scene === "rain" || scene === "storm") {
                ctx.lineWidth = 1.5;
                ctx.strokeStyle = Qt.alpha(secondary, .4);
                for (let i = 0; i < 22; i++) {
                    const x = width * (.58 + (i % 8) * .05), y = height * (.43 + Math.floor(i / 8) * .12);
                    ctx.beginPath();
                    ctx.moveTo(x, y);
                    ctx.lineTo(x - 5, y + 13);
                    ctx.stroke();
                }
                if (scene === "storm") {
                    ctx.fillStyle = Qt.alpha(accent, .7);
                    ctx.beginPath();
                    ctx.moveTo(width * .82, height * .35);
                    ctx.lineTo(width * .76, height * .52);
                    ctx.lineTo(width * .81, height * .5);
                    ctx.lineTo(width * .77, height * .65);
                    ctx.lineTo(width * .87, height * .44);
                    ctx.lineTo(width * .82, height * .46);
                    ctx.closePath();
                    ctx.fill();
                }
            } else if (scene === "snow") {
                ctx.strokeStyle = Qt.alpha(ink, .5);
                ctx.lineWidth = 1;
                for (let i = 0; i < 16; i++) {
                    const x = width * (.55 + (i % 5) * .08), y = height * (.44 + Math.floor(i / 5) * .09);
                    for (let arm = 0; arm < 3; arm++) {
                        const angle = arm * Math.PI / 3;
                        ctx.beginPath();
                        ctx.moveTo(x - Math.cos(angle) * 3, y - Math.sin(angle) * 3);
                        ctx.lineTo(x + Math.cos(angle) * 3, y + Math.sin(angle) * 3);
                        ctx.stroke();
                    }
                }
            } else if (scene === "mist") {
                for (let i = 0; i < 5; i++) {
                    const haze = ctx.createLinearGradient(width * .35, 0, width, 0);
                    haze.addColorStop(0, Qt.alpha(ink, 0));
                    haze.addColorStop(.5, Qt.alpha(ink, .09));
                    haze.addColorStop(1, Qt.alpha(ink, 0));
                    ctx.fillStyle = haze;
                    ctx.fillRect(width * .35, height * (.4 + i * .08), width * .65, 8);
                }
            }
        }
    }
}
