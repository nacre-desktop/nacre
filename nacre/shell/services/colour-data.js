.pragma library

const fallback = {
    "surface": "111213",
    "surfaceDim": "111213",
    "surfaceBright": "252727",
    "surfaceContainerLowest": "0a0b0c",
    "surfaceContainerLow": "131415",
    "surfaceContainer": "191a1a",
    "surfaceContainerHigh": "1e1f20",
    "surfaceContainerHighest": "222324",
    "frame": "192129",
    "background": "111213",
    "surfaceVariant": "222324",
    "onSurface": "eaebed",
    "onSurfaceVariant": "b0b1b2",
    "shadow": "000000",
    "scrim": "000000",
    "onBackground": "eaebed",
    "outline": "797a7c",
    "outlineVariant": "3f4041",
    "inverseSurface": "f0f2f3",
    "inverseOnSurface": "212223",
    "primary": "868e95",
    "onPrimary": "101014",
    "primaryDim": "868e95",
    "primaryContainer": "383b3d",
    "onPrimaryContainer": "e6e8ea",
    "primaryFixed": "cdd5dd",
    "primaryFixedDim": "b1b8bf",
    "onPrimaryFixed": "11171c",
    "onPrimaryFixedVariant": "353b41",
    "primaryPaletteKeyColor": "787f86",
    "primary_paletteKeyColor": "787f86",
    "secondary": "8a8e90",
    "onSecondary": "101014",
    "secondaryDim": "8b8e90",
    "secondaryContainer": "3a3b3b",
    "onSecondaryContainer": "e7e8e8",
    "secondaryFixed": "d2d5d7",
    "secondaryFixedDim": "b5b8ba",
    "onSecondaryFixed": "141618",
    "onSecondaryFixedVariant": "393b3d",
    "secondaryPaletteKeyColor": "7c7f81",
    "secondary_paletteKeyColor": "7c7f81",
    "tertiary": "8b8e8f",
    "onTertiary": "101014",
    "tertiaryDim": "8b8e90",
    "tertiaryContainer": "3a3b3b",
    "onTertiaryContainer": "e7e8e8",
    "tertiaryFixed": "d2d4d6",
    "tertiaryFixedDim": "b5b8b9",
    "onTertiaryFixed": "151617",
    "onTertiaryFixedVariant": "393b3c",
    "tertiaryPaletteKeyColor": "7d7f80",
    "tertiary_paletteKeyColor": "7d7f80",
    "error": "fb4e54",
    "onError": "101014",
    "errorDim": "fb4e54",
    "errorContainer": "5f2525",
    "onErrorContainer": "ffdfdd",
    "success": "349f6c",
    "onSuccess": "101014",
    "successDim": "359f6c",
    "successContainer": "214330",
    "onSuccessContainer": "d4f0df",
    "surfaceTint": "868e95",
    "overtone": "73808c",
    "orient1": "73808c",
    "orient2": "7a7f83",
    "orient3": "7b7f82",
    "inversePrimary": "63707c",
    "neutralPaletteKeyColor": "707273",
    "neutralVariantPaletteKeyColor": "6f7274",
    "neutral_paletteKeyColor": "707273",
    "neutral_variant_paletteKeyColor": "6f7274",
    "errorPaletteKeyColor": "df303e",
    "rosewater": "d8ada7",
    "flamingo": "dd8d81",
    "pink": "d965aa",
    "mauve": "ae75d6",
    "red": "fb4e54",
    "maroon": "d96a7e",
    "peach": "e68d50",
    "yellow": "d4b14b",
    "green": "349f6c",
    "teal": "309f95",
    "sky": "55accc",
    "sapphire": "4495c2",
    "blue": "538cee",
    "lavender": "8f84d5",
    "text": "eaebed",
    "subtext1": "b0b1b2",
    "subtext0": "b0b1b2",
    "overlay2": "797a7c",
    "overlay1": "797a7c",
    "overlay0": "3f4041",
    "surface2": "222324",
    "surface1": "1e1f20",
    "surface0": "191a1a",
    "base": "111213",
    "mantle": "0a0b0c",
    "crust": "0a0b0c",
    "klink": "538cee",
    "kvisited": "ae75d6",
    "knegative": "fb4e54",
    "kneutral": "d4b14b",
    "kpositive": "349f6c",
    "klinkSelection": "002260",
    "kvisitedSelection": "410060",
    "knegativeSelection": "55000b",
    "kneutralSelection": "312500",
    "kpositiveSelection": "002d19",
    "term0": "111213",
    "term8": "b0b1b2",
    "term1": "fb4e54",
    "term9": "fb4e54",
    "term2": "349f6c",
    "term10": "349f6c",
    "term3": "d4b14b",
    "term11": "d4b14b",
    "term4": "538cee",
    "term12": "538cee",
    "term5": "ae75d6",
    "term13": "ae75d6",
    "term6": "309f95",
    "term14": "309f95",
    "term7": "eaebed",
    "term15": "eaebed"
};
const names = ["rosewater", "flamingo", "pink", "mauve", "red", "maroon", "peach", "yellow", "green", "teal", "sky", "sapphire", "blue", "lavender"];
function roleName(key) {
    if (names.includes(key))
        return key;
    return "m3" + key.replace(/PaletteKeyColor$/, "_paletteKeyColor");
}
function prepare(data) {
    if (!data || !["dark", "light"].includes(data.mode) || !data.colours || Array.isArray(data.colours))
        return null;
    const colours = data.colours, palette = {};
    for (const key of Object.keys(fallback)) {
        const value = colours[key];
        if (typeof value !== "string" || !/^#?[0-9a-fA-F]{6}$/.test(value))
            return null;
        const hex = value.replace(/^#/, "");
        const color = Qt.rgba(parseInt(hex.slice(0, 2), 16) / 255, parseInt(hex.slice(2, 4), 16) / 255, parseInt(hex.slice(4, 6), 16) / 255, 1);
        palette[roleName(key)] = color;
        if (key === "neutralVariantPaletteKeyColor")
            palette.m3neutral_variant_paletteKeyColor = color;
        palette[key] = color;
    }
    return {
        light: data.mode === "light",
        palette: palette,
        raw: data
    };
}
function textPayload(text) {
    if (typeof text !== "string" || text.length > 2097152)
        return null;
    try {
        if (text.trim().startsWith("{"))
            return JSON.parse(text);
    } catch (failure) {
        return null;
    }
    const data = {
        mode: "dark",
        colours: {}
    };
    for (const line of text.split(/\n/)) {
        const match = /^(\w+)\s+(.+)$/.exec(line.trim());
        if (!match)
            continue;
        if (match[1] === "mode")
            data.mode = match[2];
        else if (Object.prototype.hasOwnProperty.call(fallback, match[1]))
            data.colours[match[1]] = match[2];
    }
    return data;
}
