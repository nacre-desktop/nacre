.pragma library

function fraction(value) {
    return Number.isFinite(value) ? Math.max(0, Math.min(1, value)) : 0;
}
function percent(value) {
    return Number.isFinite(value) ? Math.round(fraction(value) * 100) + "%" : "—";
}
function progress(position, duration) {
    return Number.isFinite(position) && Number.isFinite(duration) && duration > 0 ? fraction(position / duration) : 0;
}
function dayKey(date) {
    return date.getFullYear() + "-" + (date.getMonth() + 1) + "-" + date.getDate();
}
function calendar(year, month, firstWeekday) {
    const first = new Date(year, month, 1, 12);
    const offset = (first.getDay() - firstWeekday + 7) % 7;
    return Array.from({
        length: 42
    }, (_, index) => {
        const date = new Date(year, month, index - offset + 1, 12);
        return {
            day: date.getDate(),
            key: dayKey(date),
            inMonth: date.getMonth() === first.getMonth()
        };
    });
}
function osName(text) {
    const fields = {};
    for (const line of text.split("\n")) {
        const found = line.match(/^(PRETTY_NAME|NAME)=(.*)$/);
        if (found) {
            const value = found[2].trim();
            fields[found[1]] = value.replace(/^(["'])(.*)\1$/, "$2");
        }
    }
    return fields.PRETTY_NAME || fields.NAME || "Linux";
}
function uptime(seconds, translate) {
    if (!Number.isFinite(seconds) || seconds < 0)
        return translate ? translate("dashboard.uptime.unavailable", "Uptime unavailable") : "Uptime unavailable";
    const minutes = Math.floor(seconds / 60);
    const days = Math.floor(minutes / 1440);
    const args = {
        days: days,
        hours: Math.floor(minutes / 60) % 24,
        minutes: minutes % 60
    };
    const fallback = days ? "Up {days}d {hours}h {minutes}m" : "Up {hours}h {minutes}m";
    return translate ? translate(days ? "dashboard.uptime.days" : "dashboard.uptime.hours", fallback, args) : "Up " + (days ? days + "d " : "") + args.hours + "h " + args.minutes + "m";
}
function inViewport(box, top, height) {
    return height > 0 && box[1] < top + height && box[1] + box[3] > top;
}
function layout(width) {
    const w = Math.max(1, width), gap = 12;
    if (w < 420) {
        return {
            height: 1444,
            weather: [0, 0, w, 158],
            host: [0, 170, w, 158],
            clock: [0, 340, w, 170],
            calendar: [0, 522, w, 320],
            resources: [0, 854, w, 220],
            media: [0, 1086, w, 358]
        };
    }
    if (w < 700) {
        const weather = (w - gap) * 0.4;
        return {
            height: 756,
            weather: [0, 0, weather, 158],
            host: [weather + gap, 0, w - weather - gap, 158],
            clock: [0, 170, 100, 318],
            calendar: [112, 170, w - 112, 318],
            resources: [0, 500, 112, 256],
            media: [124, 500, w - 124, 256]
        };
    }
    const side = 206, main = w - side - gap, weather = (main - gap) * 0.38;
    return {
        height: 488,
        weather: [0, 0, weather, 158],
        host: [weather + gap, 0, main - weather - gap, 158],
        clock: [0, 170, 112, 318],
        calendar: [124, 170, main - 248, 318],
        resources: [main - 112, 170, 112, 318],
        media: [w - side, 0, side, 488]
    };
}
