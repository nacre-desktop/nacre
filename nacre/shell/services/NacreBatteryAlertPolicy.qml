import QtQuick

QtObject {
    id: root
    property bool initialized: false
    property int severity: 0
    property int epoch: 0
    signal rearmed

    function initialize(level) {
        if (!Number.isInteger(level) || level < 0 || level > 2)
            return false;
        severity = level;
        initialized = true;
        return true;
    }
    function reset() {
        severity = 0;
        epoch++;
        rearmed();
    }
    function request(sample) {
        if (!initialized || sample?.ready !== true)
            return null;
        if (sample.present === false) {
            reset();
            return null;
        }
        if (sample.present !== true || typeof sample.discharging !== "boolean" || !Number.isFinite(sample.fraction) || sample.fraction < 0 || sample.fraction > 1)
            return null;
        if (!sample.discharging || sample.fraction > 0.20) {
            reset();
            return null;
        }
        const level = sample.fraction <= 0.15 ? 2 : 1;
        if (level <= severity)
            return null;
        return {
            level: level,
            percent: Math.round(sample.fraction * 100),
            epoch: epoch
        };
    }
    function delivered(request) {
        if (!request || request.epoch !== epoch || !Number.isInteger(request.level) || request.level < 1 || request.level > 2)
            return false;
        severity = Math.max(severity, request.level);
        return true;
    }
}
