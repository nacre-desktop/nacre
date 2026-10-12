pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io
import "../i18n/translation.js" as Translation

Singleton {
    id: root
    readonly property string requestedLanguage: Quickshell.env("NACRE_LANGUAGE") || DesktopSettings.data.language || "system"
    readonly property string language: requestedLanguage === "pseudo" ? "pseudo" : requestedLanguage === "system" ? Qt.locale().name.split("_")[0] : /^[a-z]{2,3}(?:[-_][A-Za-z0-9]+)*$/.test(requestedLanguage) ? requestedLanguage : "en"
    readonly property bool developerMode: Quickshell.env("NACRE_DEVELOPER_MODE") === "1"
    readonly property var locale: requestedLanguage === "system" || requestedLanguage === "pseudo" ? Qt.locale() : Qt.locale(requestedLanguage)
    property var english: ({})
    property var translated: ({})
    readonly property var messages: Object.assign({}, english, translated)
    property var options: ({})
    function tr(id, fallback, args = {}) {
        return Translation.translate(messages, language, id, fallback, args);
    }
    function localize(value) {
        return Translation.localize(value, tr);
    }
    function option(domain, key) {
        const row = options[domain]?.[key];
        const fallback = ["personality", "workflow", "dashboard-tab", "mode"].includes(domain) ? String(key).charAt(0).toUpperCase() + String(key).slice(1) : key;
        const id = "options." + domain + "." + String(key).toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");
        return tr(row?.id || id, row?.fallback || fallback);
    }
    function modelLabel(row, key) {
        return row[key + "Id"] ? tr(row[key + "Id"], row[key]) : row[key];
    }
    function number(value, decimals = 0) {
        return Number(value).toLocaleString(locale, "f", decimals);
    }
    function dateTime(value, format = Locale.ShortFormat) {
        return new Date(value).toLocaleString(locale, format);
    }
    function date(value, format = Locale.ShortFormat) {
        return new Date(value).toLocaleDateString(locale, format);
    }
    function time(value, format = Locale.ShortFormat) {
        return new Date(value).toLocaleTimeString(locale, format);
    }
    FileView {
        path: Qt.resolvedUrl("../i18n/" + (root.language === "pseudo" ? "en" : root.language) + ".json")
        printErrors: false
        onLoaded: {
            try {
                root.translated = JSON.parse(text());
            } catch (error) {
                root.translated = {};
            }
        }
        onLoadFailed: root.translated = {}
    }
    FileView {
        path: Qt.resolvedUrl("../i18n/options.json")
        onLoaded: {
            try {
                root.options = JSON.parse(text());
            } catch (error) {
                root.options = {};
            }
        }
    }
    FileView {
        path: Qt.resolvedUrl("../i18n/en.json")
        onLoaded: {
            try {
                const parsed = JSON.parse(text());
                if (parsed && typeof parsed === "object" && !Array.isArray(parsed))
                    root.english = parsed;
            } catch (error) {
                root.english = {};
            }
        }
    }
}
