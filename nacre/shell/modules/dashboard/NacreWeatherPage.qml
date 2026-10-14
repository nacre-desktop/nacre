import QtQuick
import QtQuick.Controls.Basic
import qs.widgets
import qs.services

Flickable {
    id: root
    objectName: "weatherContent"
    contentWidth: width
    contentHeight: layoutHeight
    interactive: contentHeight > height
    clip: true
    boundsBehavior: Flickable.StopAtBounds
    ScrollBar.vertical: NacreScrollBar {}
    required property bool shouldUpdate
    property bool photoLease: false
    function syncPhoto() {
        const wanted = shouldUpdate && visible && NacreDashboardState.weatherPhotos === true;
        if (wanted === photoLease)
            return;
        NacreWeatherPhoto.consumers += wanted ? 1 : -1;
        photoLease = wanted;
    }
    Component.onCompleted: syncPhoto()
    Component.onDestruction: if (photoLease)
        NacreWeatherPhoto.consumers--
    onShouldUpdateChanged: syncPhoto()
    onVisibleChanged: syncPhoto()
    Connections {
        target: NacreDashboardState
        function onWeatherPhotosChanged() {
            root.syncPhoto();
        }
    }
    readonly property var weather: NacreWeather.reading
    readonly property var hours: weather.hours || []
    readonly property var days: (weather.days || []).slice(0, compact ? 7 : 10)
    readonly property bool compact: width < 680
    readonly property real leftWidth: compact ? width : width * .6
    readonly property real layoutHeight: compact ? Math.max(height, width < 430 ? 504 : 472) : height
    readonly property real heroHeight: compact ? layoutHeight * .37 : layoutHeight * (NacreDashboardState.weatherPhotos === true ? .57 : .53)
    readonly property var temperatures: [].concat(...days.map(day => [day.low, day.high])).filter(Number.isFinite)
    readonly property real low: temperatures.length ? Math.min(...temperatures) : 0
    readonly property real span: temperatures.length ? Math.max(1, Math.max(...temperatures) - low) : 1
    implicitWidth: 940
    implicitHeight: 562
    function value(number, suffix) {
        return Number.isFinite(number) ? Math.round(number) + suffix : "—";
    }
    function icon(code) {
        return !Number.isFinite(code) || code < 0 ? "cloud_off" : code <= 1 ? "sunny" : code === 2 ? "partly_cloudy_day" : code === 3 ? "cloud" : code === 45 || code === 48 ? "foggy" : code >= 95 ? "thunderstorm" : (code >= 71 && code <= 77 || code === 85 || code === 86) ? "weather_snowy" : "rainy";
    }
    NacreSurface {
        id: hero
        objectName: "weatherHero"
        radius: 20
        color: NacreTokens.raised
        width: root.leftWidth
        height: root.heroHeight
        // Only the sky needs rounded clipping; keep weather text independent
        // of the offscreen texture used to render the photo.
        NacreClip {
            anchors.fill: parent
            radius: hero.radius
            color: hero.color
            NacreWeatherSky {
                id: sky
                anchors.fill: parent
                active: root.shouldUpdate
            }
        }
        Rectangle {
            anchors.fill: parent
            radius: hero.radius
            color: "transparent"
            border.width: 1
            border.color: Qt.alpha(NacreTokens.accent, .4)
        }
        Column {
            x: 20
            y: 16
            width: parent.width * .55
            spacing: root.compact ? 3 : 6
            NacreText {
                width: parent.width
                text: NacreWeather.location || NacreI18n.tr("dashboard.weather-page.text.weather", "Weather")
                color: Qt.alpha(NacreTokens.ink, .85)
                font.pointSize: 11
                elide: Text.ElideRight
            }
            NacreText {
                text: Number.isFinite(NacreWeather.temperature) ? NacreWeather.degrees(NacreWeather.temperature) : "—"
                font.pointSize: root.compact ? 30 : 46
                font.weight: Font.Light
            }
            NacreText {
                width: parent.width
                text: NacreWeather.description || NacreI18n.tr("dashboard.weather-page.text.weather-unavailable", "Weather unavailable")
                color: NacreTokens.accent
                font.pointSize: root.compact ? 11 : 14
                elide: Text.ElideRight
            }
            NacreText {
                width: parent.width
                visible: !root.compact
                text: NacreWeather.detail + (Number.isFinite(root.weather.direction) ? " · " + NacreI18n.tr("weather.wind-direction", "{direction}° direction", {
                        direction: Math.round(root.weather.direction)
                    }) : "")
                color: Qt.alpha(NacreTokens.ink, .85)
                font.pointSize: 9
                elide: Text.ElideRight
            }
        }
        Canvas {
            id: sunArc
            x: parent.width * .6
            y: root.compact ? 31 : 45
            width: parent.width * .34
            height: root.compact ? 62 : 94
            property var reading: root.weather
            property color first: NacreTokens.accent
            onReadingChanged: requestPaint()
            onFirstChanged: requestPaint()
            onWidthChanged: requestPaint()
            onHeightChanged: requestPaint()
            onPaint: {
                const ctx = getContext("2d");
                ctx.reset();
                const r = Math.min(width / 2 - 8, height - 12), cx = width / 2, cy = height - 8;
                ctx.lineWidth = 2;
                ctx.strokeStyle = Qt.alpha(NacreTokens.accent, .45);
                ctx.beginPath();
                ctx.arc(cx, cy, r, Math.PI, Math.PI * 2);
                ctx.stroke();
                const first = root.days[0];
                if (!first || !root.weather.localTime)
                    return;
                const minute = value => Number(value.slice(11, 13)) * 60 + Number(value.slice(14, 16));
                const fraction = Math.max(0, Math.min(1, (minute(root.weather.localTime) - minute(first.sunrise)) / Math.max(1, minute(first.sunset) - minute(first.sunrise))));
                const angle = Math.PI + fraction * Math.PI;
                ctx.fillStyle = NacreTokens.accent;
                ctx.beginPath();
                ctx.arc(cx + r * Math.cos(angle), cy + r * Math.sin(angle), 5, 0, Math.PI * 2);
                ctx.fill();
            }
        }
        NacreText {
            x: sunArc.x
            y: sunArc.y + sunArc.height + 4
            width: sunArc.width
            text: root.days.length ? NacreI18n.tr("dashboard.weather-page.sun-times", "Sunrise {rise} · Sunset {set}", {
                rise: root.days[0].sunrise.slice(11, 16),
                set: root.days[0].sunset.slice(11, 16)
            }) : "—"
            horizontalAlignment: Text.AlignHCenter
            font.pointSize: root.compact ? 7 : 8
            wrapMode: Text.Wrap
            color: Qt.alpha(NacreTokens.ink, .85)
        }
        Grid {
            x: 20
            y: root.compact ? parent.height - height - 8 - (sky.photoReady ? 14 : 0) : parent.height - 118 - (sky.photoReady ? 14 : 0)
            width: parent.width - 40
            columns: root.compact && root.width >= 430 ? 6 : 3
            rowSpacing: root.compact ? 3 : 12
            Repeater {
                model: [
                    {
                        label: NacreI18n.tr("dashboard.weather-page.wind", "Wind"),
                        value: root.value(root.weather.wind, " km/h")
                    },
                    {
                        label: NacreI18n.tr("dashboard.weather-page.humidity", "Humidity"),
                        value: root.value(root.weather.humidity, "%")
                    },
                    {
                        label: NacreI18n.tr("dashboard.weather-page.uv", "UV"),
                        value: root.value(root.days[0]?.uv, "")
                    },
                    {
                        label: NacreI18n.tr("dashboard.weather-page.rain", "Rain chance"),
                        value: root.value(root.hours[0]?.rain, "%")
                    },
                    {
                        label: NacreI18n.tr("dashboard.weather-page.air-quality", "Air quality"),
                        value: root.value(root.weather.aqi, " AQI")
                    },
                    {
                        label: NacreI18n.tr("dashboard.weather-page.pressure", "Pressure"),
                        value: root.value(root.weather.pressure, " hPa")
                    }
                ]
                Column {
                    required property var modelData
                    width: parent.width / parent.columns
                    spacing: root.compact ? 1 : 3
                    NacreText {
                        text: parent.modelData.label
                        font.pointSize: root.compact ? 8 : 9
                        color: Qt.alpha(NacreTokens.ink, .85)
                    }
                    NacreText {
                        text: parent.modelData.value
                        font.pointSize: root.compact ? 10 : 13
                        font.weight: Font.DemiBold
                    }
                }
            }
        }
        NacreText {
            id: credit
            objectName: "weatherPhotoCredit"
            x: 20
            width: parent.width - 40
            anchors.bottom: parent.bottom
            anchors.bottomMargin: 6
            visible: sky.photoReady
            text: sky.photoReady ? NacreI18n.tr("dashboard.weather-photo.credit", "Photo: {author} · {license} · cropped/softened", {
                author: NacreWeatherPhoto.author,
                license: NacreWeatherPhoto.license
            }) : ""
            textFormat: Text.PlainText
            font.pointSize: 7
            color: Qt.alpha(NacreTokens.ink, .85)
            elide: Text.ElideRight
            NacreInteraction {
                accessibleName: credit.text
                function onClicked() {
                    if (sky.photoReady)
                        Qt.openUrlExternally(NacreWeatherPhoto.source);
                }
            }
        }
    }
    NacreSurface {
        id: hourly
        objectName: "weatherHourly"
        y: hero.height + 14
        width: root.leftWidth
        height: root.compact ? root.layoutHeight * .26 : root.layoutHeight - y
        radius: 16
        color: NacreTokens.raised
        readonly property var points: root.hours.filter(p => Number.isFinite(p.temperature))
        readonly property var slots: points.map((hour, index) => ({
                    hour: hour,
                    index: index
                })).filter(p => p.index % 4 === 0 || p.index === points.length - 1)
        readonly property real minimum: points.length ? Math.min(...points.map(p => p.temperature)) : 0
        readonly property real span: points.length ? Math.max(1, Math.max(...points.map(p => p.temperature)) - minimum) : 1
        readonly property real cellWidth: plot.width / Math.max(1, slots.length)
        function timeX(index) {
            return cellWidth / 2 + index / Math.max(1, points.length - 1) * (plot.width - cellWidth);
        }
        function temperatureY(value) {
            return 16 + (1 - (value - minimum) / span) * Math.max(4, plot.height - 24);
        }
        function condition(code) {
            if (!Number.isFinite(code) || code < 0)
                return NacreI18n.tr("dashboard.weather-page.condition-unavailable", "Condition unavailable");
            const key = code === 0 ? "Clear" : code === 1 ? "Mostly clear" : code === 2 ? "Partly cloudy" : code === 3 ? "Overcast" : code === 45 || code === 48 ? "Fog" : code >= 95 ? "Thunderstorm" : code >= 71 && code <= 77 || code === 85 || code === 86 ? "Snow" : code >= 80 ? "Rain showers" : code >= 61 ? "Rain" : "Drizzle";
            return NacreI18n.option("weather-description", key);
        }
        function hourIcon(hour) {
            const day = root.days.find(day => day.date === hour.time.slice(0, 10));
            const clock = hour.time.slice(11, 16), daylight = day ? clock >= day.sunrise.slice(11, 16) && clock < day.sunset.slice(11, 16) : clock >= "06:00" && clock < "20:00";
            const code = hour.code;
            if (!Number.isFinite(code) || code < 0)
                return "cloud_off";
            return code <= 1 ? daylight ? "sunny" : "bedtime" : code === 2 ? daylight ? "partly_cloudy_day" : "partly_cloudy_night" : code === 45 || code === 48 ? "foggy" : code >= 95 ? "thunderstorm" : root.icon(code);
        }
        NacreText {
            x: 16
            y: 12
            text: NacreI18n.tr("dashboard.weather-page.text.next-24-hours", "Next 24 hours")
            font.pointSize: 12
            font.weight: Font.DemiBold
        }
        NacreText {
            anchors.right: parent.right
            anchors.rightMargin: 16
            y: 16
            text: root.compact ? NacreI18n.tr("dashboard.weather-page.precipitation-chance", "Precipitation chance") : NacreI18n.tr("dashboard.weather-page.hourly-legend", "Temperature · Precipitation chance")
            font.pointSize: root.compact ? 7 : 8
            color: NacreTokens.mutedInk
            visible: parent.width >= 240
        }
        Item {
            id: plot
            objectName: "weatherTemperaturePlot"
            x: 16
            y: 42
            width: parent.width - 32
            height: Math.max(24, parent.height - y - 48)
            Canvas {
                anchors.fill: parent
                property var values: hourly.points
                property color first: NacreTokens.accent
                property color second: NacreTokens.orientSecondary
                onValuesChanged: requestPaint()
                onFirstChanged: requestPaint()
                onSecondChanged: requestPaint()
                onWidthChanged: requestPaint()
                onHeightChanged: requestPaint()
                onAvailableChanged: if (available)
                    requestPaint()
                onPaint: {
                    const ctx = getContext("2d");
                    ctx.reset();
                    if (!values.length)
                        return;
                    ctx.lineWidth = 2;
                    const temperature = ctx.createLinearGradient(0, 0, 0, height);
                    temperature.addColorStop(0, first);
                    temperature.addColorStop(1, second);
                    ctx.strokeStyle = temperature;
                    ctx.beginPath();
                    values.forEach((p, i) => {
                        if (i)
                            ctx.lineTo(hourly.timeX(i), hourly.temperatureY(p.temperature));
                        else
                            ctx.moveTo(hourly.timeX(i), hourly.temperatureY(p.temperature));
                    });
                    ctx.stroke();
                    ctx.lineTo(hourly.timeX(values.length - 1), height);
                    ctx.lineTo(hourly.timeX(0), height);
                    ctx.closePath();
                    const wash = ctx.createLinearGradient(0, 0, 0, height);
                    wash.addColorStop(0, Qt.alpha(first, .16));
                    wash.addColorStop(1, Qt.alpha(first, 0));
                    ctx.fillStyle = wash;
                    ctx.fill();
                }
            }
            Repeater {
                model: hourly.slots
                NacreText {
                    required property var modelData
                    x: hourly.timeX(modelData.index) - width / 2
                    y: Math.max(0, hourly.temperatureY(modelData.hour.temperature) - height - 3)
                    text: NacreWeather.degrees(modelData.hour.temperature)
                    font.pointSize: root.compact ? 8 : 9
                    color: NacreTokens.ink
                }
            }
        }
        Repeater {
            model: hourly.slots
            Item {
                id: hourSlot
                required property var modelData
                objectName: "weatherHour" + modelData.index
                readonly property string rain: Number.isFinite(modelData.hour.rain) ? NacreI18n.tr("dashboard.weather-page.hour-rain", "{chance}%", {
                    chance: Math.round(modelData.hour.rain)
                }) : "—"
                readonly property string summary: NacreI18n.tr("dashboard.weather-page.hour-summary", "{time}: {temperature}, {condition}, precipitation chance {chance}", {
                    time: modelData.hour.time.slice(11, 16),
                    temperature: NacreWeather.degrees(modelData.hour.temperature),
                    condition: hourly.condition(modelData.hour.code),
                    chance: rain
                })
                x: plot.x + hourly.timeX(modelData.index) - width / 2
                y: parent.height - 42
                width: Math.max(28, hourly.cellWidth * .75)
                height: 40
                Accessible.role: Accessible.StaticText
                Accessible.name: summary
                NacreIcon {
                    id: hourSymbol
                    objectName: "weatherHourIcon" + hourSlot.modelData.index
                    x: root.compact ? (parent.width - width) / 2 : (parent.width - width - 4 - chanceLabel.width) / 2
                    text: hourly.hourIcon(hourSlot.modelData.hour)
                    font.pointSize: root.compact ? 10 : 12
                    color: NacreTokens.orientSecondary
                }
                NacreText {
                    id: chanceLabel
                    x: root.compact ? (parent.width - width) / 2 : hourSymbol.x + hourSymbol.width + 4
                    y: root.compact ? 14 : 0
                    text: hourSlot.rain
                    font.pointSize: root.compact ? 7 : 8
                    color: NacreTokens.ink
                }
                NacreText {
                    anchors.horizontalCenter: parent.horizontalCenter
                    y: root.compact ? 28 : 22
                    text: hourSlot.modelData.hour.time.slice(11, 16)
                    font.pointSize: 8
                    color: NacreTokens.mutedInk
                }
                HoverHandler {
                    id: hover
                }
                ToolTip {
                    id: hourHint
                    visible: hover.hovered
                    delay: 250
                    text: hourSlot.summary
                    contentItem: NacreText {
                        text: hourHint.text
                        font.pointSize: 9
                        color: NacreTokens.ink
                    }
                    background: NacreSurface {
                        radius: 10
                        color: NacreTokens.raised
                    }
                }
            }
        }
        NacreText {
            anchors.centerIn: parent
            visible: !hourly.points.length
            text: NacreI18n.tr("dashboard.weather-page.text.forecast-unavailable", "Forecast unavailable")
            font.pointSize: 10
            color: Qt.alpha(NacreTokens.ink, .85)
        }
    }
    NacreSurface {
        id: daily
        x: root.compact ? 0 : root.leftWidth + 16
        y: root.compact ? hourly.y + hourly.height + 12 : 0
        width: parent.width - x
        height: root.layoutHeight - y
        radius: 16
        color: NacreTokens.raised
        NacreText {
            x: 16
            y: 12
            text: NacreI18n.tr("dashboard.weather-page.outlook", "Daily outlook")
            font.pointSize: 12
            font.weight: Font.DemiBold
        }
        Column {
            x: 16
            y: root.compact ? 34 : 54
            width: parent.width - 32
            Repeater {
                model: root.days
                Item {
                    required property var modelData
                    width: parent.width
                    height: Math.max(14, (daily.height - (root.compact ? 38 : 66)) / Math.max(1, root.days.length))
                    NacreText {
                        width: parent.width * .2
                        anchors.verticalCenter: parent.verticalCenter
                        text: NacreI18n.date(new Date(modelData.date + "T12:00:00"), "ddd")
                        font.pointSize: 9
                    }
                    NacreIcon {
                        x: parent.width * .21
                        anchors.verticalCenter: parent.verticalCenter
                        text: root.icon(parent.modelData.code)
                        font.pointSize: 11
                        color: NacreTokens.orientSecondary
                    }
                    NacreText {
                        x: parent.width * .32
                        anchors.verticalCenter: parent.verticalCenter
                        text: NacreWeather.degrees(parent.modelData.low)
                        font.pointSize: 9
                        color: Qt.alpha(NacreTokens.ink, .85)
                    }
                    Rectangle {
                        x: parent.width * .47
                        width: parent.width * .24
                        height: 5
                        radius: 2.5
                        anchors.verticalCenter: parent.verticalCenter
                        color: Qt.alpha(NacreTokens.accent, .1)
                        Rectangle {
                            x: parent.width * (parent.parent.modelData.low - root.low) / root.span
                            width: Math.max(3, parent.width * (parent.parent.modelData.high - parent.parent.modelData.low) / root.span)
                            height: parent.height
                            radius: 2.5
                            gradient: Gradient {
                                orientation: Gradient.Horizontal
                                GradientStop {
                                    position: 0
                                    color: NacreTokens.orientSecondary
                                }
                                GradientStop {
                                    position: 1
                                    color: NacreTokens.accent
                                }
                            }
                        }
                    }
                    NacreText {
                        x: parent.width * .75
                        anchors.verticalCenter: parent.verticalCenter
                        text: NacreWeather.degrees(parent.modelData.high)
                        font.pointSize: 9
                    }
                    NacreText {
                        anchors.right: parent.right
                        anchors.verticalCenter: parent.verticalCenter
                        text: root.value(parent.modelData.rain, "%")
                        font.pointSize: 9
                        color: Qt.alpha(NacreTokens.ink, .85)
                    }
                }
            }
        }
    }
}
