import QtQuick
import qs.widgets
import qs.services

NacreSurface {
    id: root
    property string label: ""
    property string value: ""
    property string detail: ""
    property string icon: ""
    property var history: []
    property real fraction: NaN
    property real valueSize: 0
    readonly property bool compact: height < 120
    readonly property bool prominent: height >= 190
    readonly property real gaugeSize: prominent ? Math.max(58, Math.min(156, width - 36, height - 112)) : compact ? Math.max(24, Math.min(58, height - 36)) : Math.min(88, width * .4)
    radius: 16
    color: NacreTokens.raised
    NacreIcon {
        x: 14
        y: 12
        text: root.icon
        color: NacreTokens.accent
        font.pointSize: 15
    }
    NacreText {
        x: 40
        y: 13
        width: parent.width - 82
        text: root.label
        font.pointSize: 10
        color: NacreTokens.mutedInk
        elide: Text.ElideRight
    }
    Canvas {
        id: gauge
        objectName: "resourceGauge"
        x: root.prominent ? (parent.width - width) / 2 : 12
        y: root.prominent ? 36 : root.compact ? 32 : 42
        width: root.gaugeSize
        height: width
        property real amount: root.fraction
        property color first: NacreTokens.accent
        property color second: NacreTokens.orientSecondary
        onAmountChanged: requestPaint()
        onFirstChanged: requestPaint()
        onSecondChanged: requestPaint()
        onWidthChanged: requestPaint()
        onAvailableChanged: if (available)
            requestPaint()
        onPaint: {
            const ctx = getContext("2d");
            ctx.reset();
            const c = width / 2, r = c - 6, start = Math.PI * .75, end = Math.PI * 2.25;
            ctx.lineWidth = root.prominent ? 9 : root.compact ? 5 : 7;
            ctx.lineCap = "round";
            ctx.strokeStyle = Qt.alpha(first, .14);
            ctx.beginPath();
            ctx.arc(c, c, r, start, end);
            ctx.stroke();
            if (Number.isFinite(amount) && amount > 0) {
                const gradient = ctx.createLinearGradient(0, height, width, 0);
                gradient.addColorStop(0, String(first));
                gradient.addColorStop(1, String(second));
                ctx.strokeStyle = gradient;
                ctx.beginPath();
                ctx.arc(c, c, r, start, start + (end - start) * Math.max(0, Math.min(1, amount)));
                ctx.stroke();
            }
        }
        NacreText {
            anchors.centerIn: parent
            text: Number.isFinite(root.fraction) ? NacreI18n.tr("dashboard.system-page.percent", "{value}%", {
                value: Math.round(root.fraction * 100)
            }) : "—"
            font.pointSize: root.prominent ? 24 : root.compact ? Math.max(8, Math.min(12, root.gaugeSize * .2)) : 17
            font.weight: Font.DemiBold
            color: NacreTokens.accent
        }
        Accessible.role: Accessible.Indicator
        Accessible.name: root.label + " " + (Number.isFinite(root.fraction) ? NacreI18n.tr("dashboard.system-page.percent", "{value}%", {
                value: Math.round(root.fraction * 100)
            }) : NacreI18n.tr("dashboard.system-page.gpu-unavailable", "Unavailable"))
    }
    NacreText {
        id: metricValue
        x: root.prominent ? 14 : gauge.x + gauge.width + 10
        y: root.prominent ? gauge.y + gauge.height + 10 : root.compact ? 36 : 54
        width: root.prominent ? parent.width - 28 : parent.width - x - 14
        horizontalAlignment: root.prominent ? Text.AlignHCenter : Text.AlignLeft
        text: root.value
        font.pointSize: root.valueSize || (root.prominent ? 16 : 18)
        fontSizeMode: Text.HorizontalFit
        minimumPointSize: 11
        wrapMode: root.valueSize > 0 ? Text.Wrap : Text.NoWrap
        maximumLineCount: root.valueSize > 0 && !root.prominent && root.height >= 90 ? 3 : 1
        font.weight: Font.DemiBold
        elide: Text.ElideRight
    }
    NacreText {
        x: metricValue.x
        y: metricValue.y + metricValue.height + 3
        width: metricValue.width
        text: root.detail
        visible: root.height >= 90
        horizontalAlignment: root.prominent ? Text.AlignHCenter : Text.AlignLeft
        font.pointSize: root.compact ? 8 : 9
        color: NacreTokens.mutedInk
        maximumLineCount: root.prominent ? 1 : 2
        wrapMode: Text.Wrap
        elide: Text.ElideRight
    }
    NacreDashboardSparkline {
        x: root.compact ? metricValue.x : 14
        y: parent.height - height - (root.compact ? 5 : 7)
        width: root.compact ? metricValue.width : parent.width - 28
        height: root.compact ? 6 : 12
        values: root.history
        visible: root.history.length > 0 && root.height >= 90
    }
}
