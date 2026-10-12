import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import qs.widgets
import qs.services

Flickable {
    id: root
    required property bool shouldUpdate
    readonly property var weather: NacreWeather.reading
    readonly property var hours: weather.hours || []
    readonly property var days: weather.days || []
    readonly property var temperatures: [].concat(...days.map(day => [day.low, day.high])).filter(Number.isFinite)
    readonly property real low: temperatures.length ? Math.min(...temperatures) : 0
    readonly property real span: temperatures.length ? Math.max(1, Math.max(...temperatures) - low) : 1
    implicitWidth: 940
    implicitHeight: 850
    contentWidth: width
    contentHeight: sections.height
    clip: true
    boundsBehavior: Flickable.StopAtBounds
    ScrollBar.vertical: NacreScrollBar {}
    FastScroll {
        view: root
    }
    function value(number, suffix) {
        return Number.isFinite(number) ? Math.round(number) + suffix : "—";
    }
    function icon(code) {
        return code <= 1 ? "sunny" : code <= 3 ? "cloud" : code >= 71 && code <= 86 ? "weather_snowy" : "rainy";
    }
    Column {
        id: sections
        width: root.width
        spacing: 18
        NacreDashboardHero {
            width: parent.width
            height: 250
            NacreWeatherSky {
                anchors.fill: parent
                active: root.shouldUpdate
            }
            Column {
                x: 24
                y: 24
                width: parent.width * .55
                spacing: 9
                NacreText {
                    text: NacreWeather.location || NacreI18n.tr("dashboard.weather-page.text.weather", "Weather")
                    font.pointSize: 14
                    color: NacreTokens.mutedInk
                }
                NacreText {
                    text: Number.isFinite(NacreWeather.temperature) ? NacreWeather.degrees(NacreWeather.temperature) : "—"
                    font.pointSize: 42
                    font.weight: Font.Light
                }
                NacreText {
                    text: NacreWeather.description || NacreI18n.tr("dashboard.weather-page.text.weather-unavailable", "Weather unavailable")
                    font.pointSize: 16
                    color: NacreTokens.accent
                }
                NacreText {
                    text: NacreWeather.detail + "   " + NacreWeather.range
                    width: parent.width
                    wrapMode: Text.Wrap
                    font.pointSize: 10
                    color: NacreTokens.mutedInk
                }
            }
            Canvas {
                id: sunArc
                x: parent.width * .58
                y: 112
                width: parent.width * .37
                height: 90
                property string time: root.weather.localTime || ""
                onTimeChanged: requestPaint()
                onWidthChanged: requestPaint()
                onPaint: {
                    const ctx = getContext("2d");
                    ctx.reset();
                    const r = Math.min(width / 2 - 14, 65), cx = width / 2, cy = 75;
                    const gradient = ctx.createLinearGradient(cx - r, 0, cx + r, 0);
                    gradient.addColorStop(0, String(NacreTokens.accent));
                    gradient.addColorStop(1, String(NacreTokens.orientSecondary));
                    ctx.strokeStyle = gradient;
                    ctx.lineWidth = 3;
                    ctx.beginPath();
                    ctx.arc(cx, cy, r, Math.PI, Math.PI * 2);
                    ctx.stroke();
                    const first = root.days[0];
                    if (!first || !root.weather.localTime)
                        return;
                    const hour = s => Number(s.slice(11, 13)) * 60 + Number(s.slice(14, 16));
                    const fraction = Math.max(0, Math.min(1, (hour(root.weather.localTime) - hour(first.sunrise)) / Math.max(1, hour(first.sunset) - hour(first.sunrise))));
                    const angle = Math.PI + fraction * Math.PI;
                    ctx.fillStyle = "#fffdfa";
                    ctx.beginPath();
                    ctx.arc(cx + r * Math.cos(angle), cy + r * Math.sin(angle), 6, 0, Math.PI * 2);
                    ctx.fill();
                }
            }
            NacreText {
                x: parent.width * .58
                y: 210
                width: parent.width * .37
                text: root.days.length ? NacreI18n.tr("ui.weather-page.text.value0-sunrise-sunset-value1", "{value0}   Sunrise / sunset   {value1}", {
                    value0: root.days[0].sunrise.slice(11, 16),
                    value1: root.days[0].sunset.slice(11, 16)
                }) : NacreI18n.tr("dashboard.weather-page.text.sunrise-sunset-unavailable", "Sunrise / sunset unavailable")
                font.pointSize: 9
                color: NacreTokens.mutedInk
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.Wrap
            }
        }
        NacreText {
            text: NacreI18n.tr("dashboard.weather-page.text.next-24-hours", "Next 24 hours")
            font.pointSize: 15
            color: NacreTokens.accent
        }
        Item {
            width: parent.width
            height: 156
            Canvas {
                id: curve
                anchors.fill: parent
                property var values: root.hours
                property color accent: NacreTokens.accent
                onValuesChanged: requestPaint()
                onAccentChanged: requestPaint()
                onWidthChanged: requestPaint()
                onPaint: {
                    const ctx = getContext("2d");
                    ctx.reset();
                    const points = root.hours.filter(hour => Number.isFinite(hour.temperature));
                    if (points.length < 2)
                        return;
                    const temperatures = points.map(hour => hour.temperature), low = Math.min(...temperatures), span = Math.max(1, Math.max(...temperatures) - low);
                    const gradient = ctx.createLinearGradient(0, 0, width, 0);
                    gradient.addColorStop(0, String(NacreTokens.accent));
                    gradient.addColorStop(1, String(NacreTokens.orientSecondary));
                    const y = value => 88 - (value - low) / span * 62, x = i => 18 + i * (width - 36) / (points.length - 1);
                    ctx.lineWidth = 2.5;
                    ctx.strokeStyle = gradient;
                    ctx.beginPath();
                    ctx.moveTo(x(0), y(points[0].temperature));
                    for (let i = 1; i < points.length; i++) {
                        const mid = (x(i - 1) + x(i)) / 2;
                        ctx.bezierCurveTo(mid, y(points[i - 1].temperature), mid, y(points[i].temperature), x(i), y(points[i].temperature));
                    }
                    ctx.stroke();
                    ctx.lineTo(x(points.length - 1), 105);
                    ctx.lineTo(x(0), 105);
                    ctx.closePath();
                    ctx.fillStyle = Qt.alpha(NacreTokens.accent, .07);
                    ctx.fill();
                    for (let i = 0; i < points.length; i++) {
                        ctx.fillStyle = Qt.alpha(NacreTokens.orientSecondary, .5);
                        ctx.fillRect(x(i) - 3, 132 - (points[i].rain || 0) * .22, 6, (points[i].rain || 0) * .22);
                        if (i % 4 === 0) {
                            ctx.fillStyle = NacreTokens.mutedInk;
                            ctx.font = "12px sans-serif";
                            ctx.fillText(points[i].time.slice(11, 16), x(i) - 16, 153);
                            ctx.fillStyle = NacreTokens.ink;
                            ctx.fillText(NacreWeather.degrees(points[i].temperature), x(i) - 12, y(points[i].temperature) - 9);
                        }
                    }
                }
            }
            NacreText {
                anchors.centerIn: parent
                visible: root.hours.length === 0
                text: NacreI18n.tr("dashboard.weather-page.text.forecast-unavailable", "Forecast unavailable")
                color: NacreTokens.mutedInk
            }
        }
        NacreText {
            text: NacreI18n.tr("dashboard.weather-page.text.10-day-outlook", "10-day outlook")
            font.pointSize: 15
            color: NacreTokens.accent
        }
        Repeater {
            model: root.days
            RowLayout {
                required property var modelData
                width: sections.width
                spacing: 14
                NacreText {
                    Layout.preferredWidth: 90
                    text: Qt.formatDate(new Date(parent.modelData.date + "T12:00:00"), "ddd d")
                    font.pointSize: 10
                }
                NacreIcon {
                    text: root.icon(parent.modelData.code)
                    font.pointSize: 13
                    color: NacreTokens.orientSecondary
                }
                NacreText {
                    Layout.preferredWidth: 46
                    text: NacreWeather.degrees(parent.modelData.low)
                    font.pointSize: 10
                    color: NacreTokens.mutedInk
                }
                Item {
                    Layout.fillWidth: true
                    implicitHeight: 24
                    Rectangle {
                        anchors.verticalCenter: parent.verticalCenter
                        width: parent.width
                        height: 5
                        radius: 2.5
                        color: Qt.alpha(NacreTokens.mutedInk, .1)
                    }
                    Rectangle {
                        anchors.verticalCenter: parent.verticalCenter
                        x: (parent.parent.modelData.low - root.low) / root.span * parent.width
                        width: Math.max(5, (parent.parent.modelData.high - parent.parent.modelData.low) / root.span * parent.width)
                        height: 5
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
                    Layout.preferredWidth: 48
                    text: NacreWeather.degrees(parent.modelData.high)
                    font.pointSize: 10
                }
                NacreText {
                    Layout.preferredWidth: 40
                    text: root.value(parent.modelData.rain, "%")
                    font.pointSize: 9
                    color: NacreTokens.mutedInk
                }
            }
        }
        GridLayout {
            width: parent.width
            columns: root.width < 650 ? 2 : 5
            columnSpacing: 12
            rowSpacing: 12
            Repeater {
                model: [
                    {
                        label: NacreI18n.tr("dashboard.weather-page.label.wind", "Wind"),
                        value: root.value(root.weather.wind, " km/h"),
                        note: root.value(root.weather.direction, "° direction")
                    },
                    {
                        label: NacreI18n.tr("dashboard.weather-page.label.uv", "UV"),
                        value: root.value(root.days[0]?.uv, ""),
                        note: "Peak ultraviolet index"
                    },
                    {
                        label: NacreI18n.tr("dashboard.weather-page.label.humidity", "Humidity"),
                        value: root.value(root.weather.humidity, "%"),
                        note: "Moisture in the air"
                    },
                    {
                        label: NacreI18n.tr("dashboard.weather-page.label.air-quality", "Air quality"),
                        value: root.value(root.weather.aqi, " AQI"),
                        note: "US air quality index"
                    },
                    {
                        label: NacreI18n.tr("dashboard.weather-page.label.pressure", "Pressure"),
                        value: root.value(root.weather.pressure, " hPa"),
                        note: "Local surface pressure"
                    }
                ]
                Rectangle {
                    required property var modelData
                    Layout.fillWidth: true
                    Layout.preferredHeight: 100
                    radius: 12
                    color: NacreTokens.raised
                    Column {
                        x: 12
                        y: 12
                        width: parent.width - 24
                        spacing: 7
                        NacreText {
                            text: parent.parent.modelData.label
                            font.pointSize: 10
                            color: NacreTokens.mutedInk
                        }
                        NacreText {
                            text: parent.parent.modelData.value
                            font.pointSize: 14
                            color: NacreTokens.accent
                        }
                        NacreText {
                            width: parent.width
                            text: parent.parent.modelData.note
                            font.pointSize: 8
                            wrapMode: Text.Wrap
                            color: NacreTokens.mutedInk
                        }
                    }
                }
            }
        }
        Row {
            spacing: 12
            NacreText {
                text: NacreI18n.tr("ui.weather-page.text.weather-data-open-meteo-value0", "Weather data: Open-Meteo{value0}", {
                    value0: (NacreWeather.stale ? NacreI18n.tr("dashboard.weather-page.text.cached", " · Cached") : "")
                })
                font.pointSize: 9
                color: NacreTokens.mutedInk
                anchors.verticalCenter: parent.verticalCenter
            }
            ActionButton {
                text: NacreI18n.tr("dashboard.weather-page.text.refresh", "Refresh")
                compact: true
                enabled: !NacreWeather.busy
                onClicked: NacreWeather.reload()
            }
            ActionButton {
                text: NacreI18n.tr("dashboard.weather-page.text.location", "Location")
                compact: true
                onClicked: NacrePanelState.openSettings("time")
            }
        }
        NacreText {
            width: parent.width
            visible: NacreWeather.error !== ""
            text: NacreWeather.error
            wrapMode: Text.Wrap
            font.pointSize: 9
            color: NacreTokens.mutedInk
        }
    }
}
