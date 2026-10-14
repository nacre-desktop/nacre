import QtQuick
import qs.widgets
import qs.services

Item {
    id: root
    property bool active: false
    readonly property bool day: NacreWeather.reading.isDay !== false
    readonly property int code: NacreWeather.reading.wmo ?? 0
    readonly property bool photoEnabled: NacreDashboardState.weatherPhotos === true
    readonly property bool photoReady: cityPhoto.status === Image.Ready && photoEnabled
    readonly property bool moving: active && NacreTokens.motionEnabled && !WallpaperPlayback.sleeping && !WallpaperPlayback.locked
    Image {
        id: localPhoto
        anchors.fill: parent
        source: !root.photoReady && NacreWallpapers.poster ? "file://" + NacreWallpapers.poster : ""
        fillMode: Image.PreserveAspectCrop
        sourceSize.width: 960
        sourceSize.height: 600
        asynchronous: false
    }
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
                color: Qt.alpha(NacreTokens.body, root.photoReady || localPhoto.status === Image.Ready ? .46 : 1)
            }
            GradientStop {
                position: .58
                color: Qt.alpha(NacreTokens.body, root.photoReady || localPhoto.status === Image.Ready ? .55 : 1)
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
                color: Qt.alpha(NacreTokens.body, .24)
            }
            GradientStop {
                position: 1
                color: Qt.alpha(NacreTokens.orientSecondary, .08)
            }
        }
    }
    Canvas {
        id: atmosphere
        width: parent.width + 28
        height: parent.height
        property color cloud: NacreTokens.ink
        property color light: NacreTokens.accent
        property int condition: root.code
        property bool day: root.day
        onCloudChanged: requestPaint()
        onLightChanged: requestPaint()
        onConditionChanged: requestPaint()
        onDayChanged: requestPaint()
        onWidthChanged: requestPaint()
        onHeightChanged: requestPaint()
        onAvailableChanged: if (available)
            requestPaint()
        SequentialAnimation on x {
            running: root.moving && !root.photoReady && root.code >= 2
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
            const glow = ctx.createRadialGradient(width * .8, height * .15, 0, width * .8, height * .15, width * .45);
            glow.addColorStop(0, Qt.alpha(light, day ? .13 : .07));
            glow.addColorStop(1, Qt.alpha(light, 0));
            ctx.fillStyle = glow;
            ctx.fillRect(0, 0, width, height);
            if (condition < 2)
                return;
            for (let i = 0; i < 6; i++) {
                const r = width * .16, x = width * (.2 + i * .18), y = height * (.1 + (i % 2) * .1);
                ctx.save();
                ctx.translate(x, y);
                ctx.scale(1, .5);
                const haze = ctx.createRadialGradient(0, 0, 0, 0, 0, r);
                haze.addColorStop(0, Qt.alpha(cloud, .035));
                haze.addColorStop(1, Qt.alpha(cloud, 0));
                ctx.fillStyle = haze;
                ctx.fillRect(-r, -r, r * 2, r * 2);
                ctx.restore();
            }
        }
    }
}
