pragma Singleton
import QtQuick

QtObject {
    property var nodes: ({
            values: []
        })
    property var defaultAudioSink: null
    property var defaultAudioSource: null
    property var preferredDefaultAudioSink: null
}
