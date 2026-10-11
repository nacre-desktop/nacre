pragma Singleton
import QtQuick

QtObject {
    property bool light: false
    property var palette: ({
            "m3onSurface": "white",
            "m3onSurfaceVariant": "gray",
            "m3primary": "cyan",
            "m3secondary": "blue",
            "m3tertiary": "green",
            "m3surface": "black",
            "m3frame": "#232323",
            "m3outline": "gray",
            "m3outlineVariant": "gray",
            "m3onPrimary": "black",
            "m3secondaryContainer": "#333333",
            "m3surfaceContainer": "#222222",
            "m3surfaceContainerHigh": "#333333",
            "m3surfaceContainerHighest": "#444444",
            "m3error": "red"
        })

    function setMode(mode) {
    }
}
