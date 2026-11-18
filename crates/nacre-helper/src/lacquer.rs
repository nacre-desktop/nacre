//! Lacquer and inlay colour recipe: a pure post-step on Orient's role map (ported from the
//! design prototype's `themeVars`/`reach`, Final.dc.html). Surfaces take the wallpaper's main hue
//! at the locked MEDIUM tint (about 10 % lightness in dark, 95 % in light), ink is pearl or plum,
//! and every text accent keeps its hue while its lightness moves until it reads on all nine
//! surfaces.
use crate::Result;
use orient_core::{
    accessibility::{audit, BACKGROUNDS, TEXT_ROLES},
    colour::{clean, contrast, foreground},
};
use serde_json::{json, Map, Value};

const PEARL: &str = "f2edf0";
const PLUM: &str = "2a1f2b";
const WHITE: &str = "ffffff";
const SIGNAL: &str = "ff8a80";
const SIGNAL_LIGHT: &str = "e0485b";
/// Constant inlay colours exposed to Luau (same in both themes).
const ORIENT: [(&str, &str); 5] = [
    ("orientChampagne", "f0e2bb"),
    ("orientRose", "f3c7d7"),
    ("orientLilac", "d4c6f3"),
    ("orientIce", "b9d5f3"),
    ("orientMint", "bfe9d9"),
];
const TEXT_RATIO: f64 = 4.6;
const ICON_RATIO: f64 = 3.6;

fn channels(hex: &str) -> [f64; 3] {
    std::array::from_fn(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap_or(0) as f64)
}

/// `a` moved `t` of the way to `b`, rounded per channel like the prototype.
pub fn mix(a: &str, b: &str, t: f64) -> String {
    let (a, b) = (channels(a), channels(b));
    (0..3)
        .map(|i| format!("{:02x}", (a[i] + (b[i] - a[i]) * t).round() as u8))
        .collect()
}

fn to_hls(hex: &str) -> [f64; 3] {
    let [r, g, b] = channels(hex).map(|v| v / 255.0);
    let (mx, mn) = (r.max(g).max(b), r.min(g).min(b));
    let l = (mx + mn) / 2.0;
    if mx == mn {
        return [0.0, l, 0.0];
    }
    let d = mx - mn;
    let s = if l <= 0.5 {
        d / (mx + mn)
    } else {
        d / (2.0 - mx - mn)
    };
    let h = if mx == r {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if mx == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    [h / 6.0, l, s]
}

fn from_hls([h, l, s]: [f64; 3]) -> String {
    let channel = |m1: f64, m2: f64, h: f64| {
        let h = h.rem_euclid(1.0);
        if h < 1.0 / 6.0 {
            m1 + (m2 - m1) * h * 6.0
        } else if h < 0.5 {
            m2
        } else if h < 2.0 / 3.0 {
            m1 + (m2 - m1) * (2.0 / 3.0 - h) * 6.0
        } else {
            m1
        }
    };
    let rgb = if s == 0.0 {
        [l; 3]
    } else {
        let m2 = if l <= 0.5 {
            l * (1.0 + s)
        } else {
            l + s - l * s
        };
        let m1 = 2.0 * l - m2;
        [h + 1.0 / 3.0, h, h - 1.0 / 3.0].map(|h| channel(m1, m2, h))
    };
    rgb.iter()
        .map(|v| format!("{:02x}", (v.clamp(0.0, 1.0) * 255.0).round() as u8))
        .collect()
}

fn reads(color: &str, backgrounds: &[&str], ratio: f64) -> bool {
    backgrounds
        .iter()
        .all(|bg| contrast(color, bg).is_ok_and(|c| c >= ratio))
}

/// Moves a colour's lightness toward the ink side (`down` darkens, for light themes), keeping
/// its hue, until it reaches `ratio` on every background (the prototype's `reach`).
pub fn reach(color: &str, backgrounds: &[&str], down: bool, ratio: f64) -> String {
    let [h, mut l, s] = to_hls(color);
    let s = if down { (s * 1.15).min(1.0) } else { s };
    let mut out = color.to_owned();
    for _ in 0..60 {
        if reads(&out, backgrounds, ratio) {
            break;
        }
        l = if down {
            (l - 0.02).max(0.0)
        } else {
            (l + 0.02).min(1.0)
        };
        out = from_hls([h, l, s]);
    }
    out
}

/// The MEDIUM tint (Final's `tinted`): the first of `hues` with real saturation sets the hue;
/// returns (surface, frame) at fixed lightness, saturation scaled by the wallpaper's own.
fn tint(hues: &[String], light: bool) -> (String, String) {
    let hue = hues.iter().map(|hex| to_hls(hex)).find(|q| q[2] >= 0.12);
    let Some([h, _, s]) = hue else {
        let grey = if light {
            ("f8f5f6", "efeaed")
        } else {
            ("141215", "100e11")
        };
        return (grey.0.into(), grey.1.into());
    };
    let (l, sat, drop) = if light {
        (0.952, 0.42, 0.035)
    } else {
        (0.105, 0.34, 0.02)
    };
    let sat = sat * (s / 0.35).min(1.0);
    (from_hls([h, l, sat]), from_hls([h, l - drop, sat * 0.9]))
}

/// The wallpaper's shadow and highlight tones: Orient's dominant colours of the darkest and the
/// lightest quarter of the image.
pub fn tones(source: &Value) -> Option<(String, String)> {
    let tone = |name: &str| clean(source["body"][name]["hex"].as_str()?).ok();
    Some((tone("dark")?, tone("light")?))
}

fn put(colours: &mut Map<String, Value>, name: &str, value: &str) {
    colours.insert(name.into(), json!(value));
}

fn role(colours: &Map<String, Value>, name: &str) -> Option<String> {
    colours.get(name)?.as_str().map(str::to_owned)
}

/// Rewrites the surface, ink and accent roles of `colours` in place.
pub fn recolour(colours: &mut Map<String, Value>, light: bool, highlight: &str) {
    let accent = |name| role(colours, name);
    let mut hues: Vec<String> = ["primary", "secondary"]
        .iter()
        .filter_map(|n| accent(n))
        .collect();
    hues.extend(accent("tertiary"));
    if light {
        hues.insert(0, highlight.into());
    }
    let (surface, frame) = tint(&hues, light);
    let raise = mix(&surface, WHITE, if light { 0.5 } else { 0.10 });
    let ink = if light { PLUM } else { PEARL };
    let ink2 = mix(&surface, ink, 0.76);
    let ink3 = mix(&surface, ink, if light { 0.66 } else { 0.70 });
    let toward_ink = |t: f64| mix(&surface, ink, t);
    let signal = if light {
        reach(SIGNAL_LIGHT, &[&surface], true, TEXT_RATIO)
    } else {
        mix(
            SIGNAL,
            &role(colours, "tertiary").unwrap_or_else(|| SIGNAL.into()),
            0.15,
        )
    };
    let lift = |name: &str, white: f64| {
        let base = role(colours, name).unwrap_or_else(|| ink.into());
        let base = if light {
            base
        } else {
            mix(&base, WHITE, white)
        };
        reach(
            &base,
            &[&surface],
            light,
            if name == "tertiary" {
                TEXT_RATIO
            } else {
                ICON_RATIO
            },
        )
    };
    let (icon, clock) = (lift("secondary", 0.3), lift("tertiary", 0.2));
    let ladder: [(&str, String); 9] = if light {
        [
            ("surfaceContainerLowest", mix(&surface, WHITE, 0.7)),
            ("surfaceContainerLow", raise.clone()),
            ("surfaceContainer", surface.clone()),
            ("surfaceContainerHigh", toward_ink(0.04)),
            ("surfaceContainerHighest", toward_ink(0.08)),
            ("surfaceDim", toward_ink(0.08)),
            ("surfaceBright", mix(&surface, WHITE, 0.7)),
            ("surface", surface.clone()),
            ("frame", frame),
        ]
    } else {
        [
            ("surfaceContainerLowest", frame.clone()),
            ("surfaceContainerLow", surface.clone()),
            ("surfaceContainer", raise.clone()),
            ("surfaceContainerHigh", mix(&surface, WHITE, 0.14)),
            ("surfaceContainerHighest", mix(&surface, WHITE, 0.18)),
            ("surfaceDim", surface.clone()),
            ("surfaceBright", mix(&surface, WHITE, 0.22)),
            ("surface", surface.clone()),
            ("frame", frame),
        ]
    };
    for (name, value) in &ladder {
        put(colours, name, value);
    }
    for (name, value) in [
        ("background", &surface),
        ("surfaceVariant", &ladder[4].1),
        ("onSurface", &ink.to_owned()),
        ("onBackground", &ink.to_owned()),
        ("onSurfaceVariant", &ink2),
        ("outline", &ink3),
        (
            "outlineVariant",
            &toward_ink(if light { 0.2 } else { 0.22 }),
        ),
        ("error", &signal),
        ("ink", &ink.to_owned()),
        ("ink2", &ink2),
        ("ink3", &ink3),
        ("signal", &signal),
        ("raised", &raise),
        ("icon", &icon),
        ("clock", &clock),
    ] {
        put(colours, name, value);
    }
    for (name, value) in ORIENT {
        put(colours, name, value);
    }
    for (name, from) in [
        ("base", "surface"),
        ("mantle", "surfaceContainerLowest"),
        ("crust", "surfaceContainerLowest"),
        ("surface0", "surfaceContainer"),
        ("surface1", "surfaceContainerHigh"),
        ("surface2", "surfaceContainerHighest"),
        ("text", "onSurface"),
        ("subtext1", "onSurfaceVariant"),
        ("subtext0", "onSurfaceVariant"),
        ("overlay2", "outline"),
        ("overlay1", "outline"),
        ("overlay0", "outlineVariant"),
    ] {
        let value = role(colours, from).unwrap_or_default();
        put(colours, name, &value);
    }
    // Accents: same hue, lightness moved until they read on every surface.
    let surfaces: Vec<String> = BACKGROUNDS
        .iter()
        .filter_map(|n| role(colours, n))
        .collect();
    let surfaces: Vec<&str> = surfaces.iter().map(String::as_str).collect();
    for name in TEXT_ROLES.iter().filter(|n| **n != "onSurface") {
        if let Some(value) = role(colours, name) {
            put(colours, name, &reach(&value, &surfaces, light, TEXT_RATIO));
        }
    }
    for accent in ["primary", "secondary", "tertiary", "error", "success"] {
        if let Some(value) = role(colours, accent) {
            let on = format!("on{}{}", accent[..1].to_uppercase(), &accent[1..]);
            put(
                colours,
                &on,
                &foreground(&value).unwrap_or_else(|_| ink.into()),
            );
        }
    }
}

/// The post-step on a whole Orient palette document (`mode`, `source`, `colours`).
pub fn apply(palette: &mut Value) -> Result<()> {
    let Some((_, highlight)) = tones(&palette["source"]) else {
        return Ok(());
    };
    let light = palette["mode"] == "light";
    let colours = palette["colours"]
        .as_object_mut()
        .ok_or("Invalid palette colors")?;
    recolour(colours, light, &highlight);
    palette["accessibility"] = audit(colours)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The 16 wallpapers the designer checked: accent 1-3, shadow tone, highlight tone.
    const DESIGNER: [[&str; 5]; 16] = [
        ["9ED37A", "CDB8E6", "E9B7CB", "3A3448", "EEE8F2"],
        ["67BAFB", "9CC3F0", "E6C9A5", "1F3F5A", "E6D9C4"],
        ["F07A86", "F3B3C0", "E7A0B6", "2A1420", "F3D3D8"],
        ["9DBFEA", "7FA9DA", "C9DCF3", "0A1B3A", "DCE8F7"],
        ["EBC2D6", "B9A6D0", "9FCFB3", "1E2C25", "F5E4EC"],
        ["E3C4B4", "A9B2CA", "D7C29C", "5D5E6E", "F2EAE2"],
        ["E5C9DF", "C99AE0", "E8B48A", "242535", "F0E4EE"],
        ["E3B48F", "A9C6DC", "8FB0C4", "346383", "EEF1F4"],
        ["8FAEE0", "B2C4EA", "C8D4EE", "13304D", "DDE5F5"],
        ["B6C79A", "A8B9CF", "D2BE94", "505C52", "E5E7ED"],
        ["F0A774", "E6B9A2", "C9B3BA", "503026", "E6DCD8"],
        ["F0BBA6", "C9A0E0", "D8A4C8", "231639", "EDE3FA"],
        ["F0A58F", "BACBDE", "E9C8A0", "302C37", "F2E6E8"],
        ["7FD4C8", "A8D2CC", "E8CFA0", "0B1921", "DDF0EC"],
        ["8FB4E8", "A9BEDC", "E8C9A0", "121B2D", "DCE6F5"],
        ["E8B98A", "C9B6A0", "E8A0A0", "1D2125", "EEE4D8"],
    ];

    /// A role map with the roles the recipe reads, all starting at mid-grey so the fit has work.
    fn roles(accents: &[&str]) -> Map<String, Value> {
        let mut colours: Map<String, Value> = TEXT_ROLES
            .iter()
            .chain(&["success", "onPrimary"])
            .map(|name| (name.to_string(), json!("808080")))
            .collect();
        for (name, value) in ["primary", "secondary", "tertiary"].iter().zip(accents) {
            colours.insert(name.to_string(), json!(value.to_lowercase()));
        }
        colours
    }

    fn get(colours: &Map<String, Value>, name: &str) -> String {
        colours[name].as_str().unwrap().to_owned()
    }

    fn ratio(colours: &Map<String, Value>, a: &str, b: &str) -> f64 {
        contrast(&get(colours, a), &get(colours, b)).unwrap()
    }

    #[test]
    fn contrast_guarantees_hold_for_the_designer_palettes_in_both_themes() {
        for p in DESIGNER {
            for light in [false, true] {
                let mut c = roles(&p[..3]);
                recolour(&mut c, light, &p[4].to_lowercase());
                let tag = format!("{p:?} light={light}");
                for bg in BACKGROUNDS {
                    for fg in TEXT_ROLES {
                        assert!(ratio(&c, fg, bg) >= 4.5, "{fg} on {bg}: {tag}");
                    }
                }
                for fg in ["primary", "secondary", "tertiary", "clock"] {
                    assert!(ratio(&c, fg, "surface") >= 4.6, "{fg}: {tag}");
                }
                assert!(ratio(&c, "icon", "surface") >= 3.6, "icon: {tag}");
                // the designer's 4.8 / 4.78 are two-decimal roundings
                let (floor, on) = if light {
                    (4.795, "surface")
                } else {
                    (4.775, "surfaceContainer")
                };
                assert!(ratio(&c, "ink3", on) >= floor, "ink3: {tag}");
                assert!(ratio(&c, "onPrimary", "primary") >= 4.5, "onPrimary: {tag}");
            }
        }
    }

    #[test]
    fn accents_keep_their_hue_and_output_is_deterministic() {
        let mut a = roles(&["67BAFB", "9CC3F0", "E6C9A5"]);
        recolour(&mut a, true, "e6d9c4");
        let mut b = roles(&["67BAFB", "9CC3F0", "E6C9A5"]);
        recolour(&mut b, true, "e6d9c4");
        assert_eq!(a, b);
        let hue = |hex: &str| to_hls(hex)[0];
        assert!((hue(&get(&a, "primary")) - hue("67bafb")).abs() < 0.02);
        assert_eq!(get(&a, "orientChampagne"), "f0e2bb");
    }

    #[test]
    fn medium_tint_lands_near_ten_percent_dark_and_ninety_five_light() {
        for p in DESIGNER {
            for (light, want) in [(false, 0.105), (true, 0.952)] {
                let mut c = roles(&p[..3]);
                recolour(&mut c, light, &p[4].to_lowercase());
                let l = to_hls(&get(&c, "surface"))[1];
                assert!(
                    (l - want).abs() < 0.01,
                    "{p:?} light={light}: surface L {l}"
                );
                assert!(
                    (to_hls(&get(&c, "frame"))[1] - (want - if light { 0.035 } else { 0.02 }))
                        .abs()
                        < 0.01
                );
            }
        }
        // the surface carries the wallpaper's hue: blue accents tint blue
        let mut c = roles(&["67BAFB", "9CC3F0", "E6C9A5"]);
        recolour(&mut c, false, "e6d9c4");
        assert!((to_hls(&get(&c, "surface"))[0] - to_hls("67bafb")[0]).abs() < 0.02);
    }

    #[test]
    fn export_carries_the_v6_tokens_in_both_modes() {
        const TOKENS: [&str; 17] = [
            "surface",
            "raised",
            "ink",
            "ink2",
            "ink3",
            "orientChampagne",
            "orientRose",
            "orientLilac",
            "orientIce",
            "orientMint",
            "primary",
            "secondary",
            "tertiary",
            "icon",
            "clock",
            "signal",
            "frame",
        ];
        for light in [false, true] {
            let mut palette = json!({
                "mode": if light { "light" } else { "dark" },
                "source": {"body": {"dark": {"hex": "3a3448"}, "light": {"hex": "eee8f2"}}},
                "colours": roles(&["9ED37A", "CDB8E6", "E9B7CB"]),
            });
            apply(&mut palette).unwrap();
            for name in TOKENS {
                let hex = palette["colours"][name].as_str().unwrap_or_default();
                assert!(
                    u32::from_str_radix(hex, 16).is_ok() && hex.len() == 6,
                    "{name} light={light}: {hex:?}"
                );
            }
            let c = palette["colours"].as_object().unwrap();
            let raised = if light {
                "surfaceContainerLow"
            } else {
                "surfaceContainer"
            };
            assert_eq!(get(c, "raised"), get(c, raised));
        }
    }

    #[test]
    fn tones_come_from_the_darkest_and_lightest_body_colours() {
        let source = json!({"body": {"dark": {"hex": "#3A3448"}, "light": {"hex": "eee8f2"}}});
        assert_eq!(tones(&source), Some(("3a3448".into(), "eee8f2".into())));
        assert_eq!(tones(&json!({})), None);
    }
}
