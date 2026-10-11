pragma Singleton
import QtQuick

QtObject {
    property var font: ({
            "family": {
                "sans": "Sans Serif"
            }
        })
    property var anim: ({
            "durations": {
                "small": 0
            },
            "curves": {
                "standard": [0.2, 0, 0, 1, 1, 1]
            }
        })
}
