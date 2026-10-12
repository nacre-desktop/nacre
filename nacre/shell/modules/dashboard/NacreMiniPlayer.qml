import QtQuick
import qs.widgets
import qs.services
import "overview" as Cards

Cards.NacreMediaCard {
    compact: true
    audioHalo: true
    spectrum: NacreMediaDetails.spectrum
    color: NacreTokens.raised
}
