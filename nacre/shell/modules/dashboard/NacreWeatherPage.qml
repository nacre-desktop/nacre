import QtQuick
import qs.widgets
import qs.services

Item {
    id: root
    required property bool shouldUpdate
    readonly property var weather: NacreWeather.reading
    readonly property var hours: weather.hours || []
    readonly property var days: (weather.days || []).slice(0, compact ? 7 : 10)
    readonly property bool compact: width < 680
    readonly property real leftWidth: compact ? width : width * .6
    readonly property real heroHeight: compact ? height * .37 : height * .53
    readonly property var temperatures: [].concat(...days.map(day => [day.low, day.high])).filter(Number.isFinite)
    readonly property real low: temperatures.length ? Math.min(...temperatures) : 0
    readonly property real span: temperatures.length ? Math.max(1, Math.max(...temperatures) - low) : 1
    implicitWidth: 940
    implicitHeight: 562
    function value(number, suffix) {
        return Number.isFinite(number) ? Math.round(number) + suffix : "—";
    }
    function icon(code) {
        return code <= 1 ? "sunny" : code <= 3 ? "cloud" : code >= 71 && code <= 86 ? "weather_snowy" : "rainy";
    }
    NacreDashboardHero {
        id: hero
        width: root.leftWidth
        height: root.heroHeight
        NacreWeatherSky {
            anchors.fill: parent
            active: root.shouldUpdate
        }
        Column {
            x: 20
            y: 16
            width: parent.width * .55
            spacing: root.compact ? 3 : 6
            NacreText {
                width: parent.width
                text: NacreWeather.location || NacreI18n.tr("dashboard.weather-page.text.weather", "Weather")
                color: NacreTokens.mutedInk
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
                color: NacreTokens.mutedInk
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
            text: root.days.length ? root.days[0].sunrise.slice(11, 16) + "  /  " + root.days[0].sunset.slice(11, 16) : "—"
            horizontalAlignment: Text.AlignHCenter
            font.pointSize: 9
            color: NacreTokens.mutedInk
        }
        Grid {
            x: 20
            y: root.compact ? parent.height - (root.width >= 430 ? 45 : 88) : parent.height - 118
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
                        color: NacreTokens.mutedInk
                    }
                    NacreText {
                        text: parent.modelData.value
                        font.pointSize: root.compact ? 10 : 13
                        font.weight: Font.DemiBold
                    }
                }
            }
        }
    }
    NacreSurface {
        id: hourly
        y: hero.height + 14
        width: root.leftWidth
        height: root.compact ? root.height * .26 : root.height - y
        radius: 16
        color: NacreTokens.raised
        NacreText {
            x: 16
            y: 12
            text: NacreI18n.tr("dashboard.weather-page.text.next-24-hours", "Next 24 hours")
            font.pointSize: 12
            font.weight: Font.DemiBold
        }
        Canvas {
            x: 16
            y: 44
            width: parent.width - 32
            height: parent.height - 50
            property var values: root.hours
            property color first: NacreTokens.accent
            onValuesChanged: requestPaint()
            onFirstChanged: requestPaint()
            onWidthChanged: requestPaint()
            onHeightChanged: requestPaint()
            onPaint: {
                const ctx = getContext("2d");
                ctx.reset();
                const points = root.hours.filter(p => Number.isFinite(p.temperature));
                if (points.length < 2)
                    return;
                const low = Math.min(...points.map(p => p.temperature)), span = Math.max(1, Math.max(...points.map(p => p.temperature)) - low);
                const x = i => 12 + i * (width - 24) / (points.length - 1), y = value => 12 + (1 - (value - low) / span) * height * .38;
                ctx.lineWidth = 2;
                ctx.strokeStyle = NacreTokens.accent;
                ctx.beginPath();
                points.forEach((p, i) => {
                    if (i)
                        ctx.lineTo(x(i), y(p.temperature));
                    else
                        ctx.moveTo(x(i), y(p.temperature));
                });
                ctx.stroke();
                ctx.lineTo(x(points.length - 1), height * .58);
                ctx.lineTo(x(0), height * .58);
                ctx.closePath();
                const wash = ctx.createLinearGradient(0, 0, 0, height * .58);
                wash.addColorStop(0, Qt.alpha(first, .14));
                wash.addColorStop(1, Qt.alpha(first, 0));
                ctx.fillStyle = wash;
                ctx.fill();
                ctx.font = (root.compact ? "8px " : "10px ") + JSON.stringify(NacreTokens.textFamily);
                for (let i = 0; i < points.length; i++) {
                    ctx.fillStyle = Qt.alpha(NacreTokens.orientSecondary, .65);
                    ctx.fillRect(x(i) - 3, height - 24 - (points[i].rain || 0) / 100 * height * .2, 6, (points[i].rain || 0) / 100 * height * .2);
                    if (i % 4 === 0) {
                        ctx.fillStyle = NacreTokens.ink;
                        ctx.fillText(NacreWeather.degrees(points[i].temperature), x(i) - 10, Math.max(10, y(points[i].temperature) - 8));
                        ctx.fillStyle = NacreTokens.mutedInk;
                        ctx.fillText(points[i].time.slice(11, 16), x(i) - 10, root.compact ? height - 2 : height - 7);
                        ctx.fillText((points[i].rain || 0) + "%", x(i) - 10, root.compact ? height - 15 : height - 31 - (points[i].rain || 0) / 100 * height * .2);
                    }
                }
            }
        }
        NacreText {
            anchors.centerIn: parent
            visible: !root.hours.length
            text: NacreI18n.tr("dashboard.weather-page.text.forecast-unavailable", "Forecast unavailable")
            font.pointSize: 10
            color: NacreTokens.mutedInk
        }
    }
    NacreSurface {
        id: daily
        x: root.compact ? 0 : root.leftWidth + 16
        y: root.compact ? hourly.y + hourly.height + 12 : 0
        width: parent.width - x
        height: parent.height - y
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
                        color: NacreTokens.mutedInk
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
                            color: NacreTokens.accent
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
                        color: NacreTokens.mutedInk
                    }
                }
            }
        }
    }
}
