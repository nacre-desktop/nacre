pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io
import qs.utils
import "weather-photo.js" as Data

Singleton {
    id: root
    property int consumers: 0
    readonly property bool active: consumers > 0 && NacreDashboardState.weatherPhotos && !WallpaperPlayback.sleeping && !WallpaperPlayback.locked
    readonly property string city: NacreWeather.location.trim().slice(0, 100)
    readonly property string target: Qt.md5(city + "|" + NacreWeather.reading.latitude + "|" + NacreWeather.reading.longitude)
    readonly property string directory: NacrePaths.cache.slice(7) + "/weather-photo"
    readonly property string photoPath: directory + "/city.image"
    property string image: ""
    property string author: ""
    property string license: ""
    property string source: ""
    property string message: ""
    property bool cacheReady: false
    property var cache: ({})
    property var pending: null
    property string requestedCity: ""
    property string stage: ""
    property string output: ""
    property string attemptedCity: ""
    property bool cancelPending: false
    function load() {
        if (!active || !cacheReady || !city || worker.running || cancelPending || attemptedCity === city)
            return;
        if (Data.cached(cache, city, Date.now() / 1000, target)) {
            publish(cache);
            return;
        }
        attemptedCity = city;
        requestedCity = city;
        stage = "directory";
        output = "";
        worker.command = ["mkdir", "-p", "--", directory];
        worker.running = true;
    }
    function publish(data) {
        image = "file://" + photoPath;
        author = data.author;
        license = data.license;
        source = data.source;
        message = "";
    }
    function fetch(url, next, binary = false) {
        stage = next;
        output = "";
        const args = ["curl", "-q", "--proto", "=https", "--max-redirs", "0", "--fail", "--silent", "--show-error", "--connect-timeout", "3", "--max-time", "10", "--max-filesize", binary ? "2097152" : "262144", "--user-agent", "Nacre/1.0 (weather photos)"];
        worker.command = [...args, ...(binary ? ["--output", photoPath + ".part"] : ["--header", "Accept: application/json"]), "--url", url];
        worker.running = true;
    }
    function fail() {
        pending = null;
        stage = "";
        message = NacreI18n.tr("dashboard.weather-photo.unavailable", "City photo unavailable; using local imagery.");
    }
    function finish(code) {
        if (cancelPending) {
            cancelPending = false;
            Qt.callLater(load);
            return;
        }
        if (!active || requestedCity !== city) {
            stage = "";
            pending = null;
            Qt.callLater(load);
            return;
        }
        if (code !== 0) {
            fail();
            return;
        }
        if (stage === "directory") {
            fetch("https://en.wikipedia.org/w/api.php?action=query&format=json&formatversion=2&prop=pageimages%7Ccoordinates&coprimary=primary&colimit=1&redirects=1&pilicense=free&piprop=name&titles=" + encodeURIComponent(city), "city");
            return;
        }
        if (stage === "download") {
            stage = "signature";
            output = "";
            worker.command = ["od", "-An", "-tx1", "-N8", "--", photoPath + ".part"];
            worker.running = true;
            return;
        }
        if (stage === "signature") {
            const magic = output.replace(/\s+/g, "").toLowerCase();
            if (!magic.startsWith("ffd8ff") && magic !== "89504e470d0a1a0a") {
                fail();
                return;
            }
            stage = "install";
            worker.command = ["mv", "-f", "--", photoPath + ".part", photoPath];
            worker.running = true;
            return;
        }
        if (stage === "install") {
            cache = Object.assign({}, pending, {
                city: city,
                target: target,
                checked: Date.now() / 1000
            });
            pending = null;
            cacheFile.setText(JSON.stringify(cache));
            publish(cache);
            stage = "";
            return;
        }
        try {
            if (output.length > 262144)
                throw new Error();
            const data = JSON.parse(output);
            if (stage === "city") {
                const title = Data.cityImage(data, NacreWeather.reading.latitude, NacreWeather.reading.longitude);
                if (!title)
                    throw new Error();
                fetch("https://commons.wikimedia.org/w/api.php?action=query&format=json&formatversion=2&prop=imageinfo&iiprop=url%7Csize%7Cmime%7Cextmetadata&iiurlwidth=960&iiextmetadatafilter=Artist%7CAttribution%7CLicenseShortName&titles=" + encodeURIComponent("File:" + title), "metadata");
            } else if (stage === "metadata") {
                pending = Data.metadata(data);
                if (!pending)
                    throw new Error();
                fetch(pending.url, "download", true);
            }
        } catch (error) {
            fail();
        }
    }
    onActiveChanged: {
        if (active) {
            attemptedCity = "";
            load();
        } else {
            cancelPending = worker.running;
            worker.running = false;
            requestedCity = "";
            stage = "";
            pending = null;
        }
    }
    onTargetChanged: {
        cancelPending = worker.running;
        worker.running = false;
        attemptedCity = "";
        requestedCity = "";
        image = "";
        author = "";
        license = "";
        source = "";
        Qt.callLater(load);
    }
    FileView {
        id: cacheFile
        path: NacrePaths.cache + "/weather-photo.json"
        atomicWrites: true
        printErrors: false
        onLoaded: {
            try {
                const value = text();
                root.cache = value.length <= 12000 ? JSON.parse(value) : {};
            } catch (error) {
                root.cache = {};
            }
            root.cacheReady = true;
            root.load();
        }
        onLoadFailed: {
            root.cacheReady = true;
            root.load();
        }
    }
    Process {
        id: worker
        stdout: StdioCollector {
            onStreamFinished: root.output = text
        }
        onExited: code => Qt.callLater(() => root.finish(code))
    }
}
