// SPDX-License-Identifier: GPL-3.0-only
//! Nacre's theme values at the framework boundary.
use serde_json::json;

pub(crate) fn init() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| apply(&crate::prefs::Prefs::default()));
}

pub(crate) fn apply(prefs: &crate::prefs::Prefs) {
    let mut tokens: serde_json::Value =
        serde_json::from_str(include_str!("../../../shell/size-scale.json"))
            .expect("Nacre size scale is valid JSON");
    // Keep authored family names identical to the original product. User font
    // choices still flow through TextStyle aliases; static fallback scenes keep
    // their original faces rather than gaining a second alias layer here.
    tokens["fonts"] = json!({
        "sans": crate::prefs::INTERFACE_FONT,
        "mono": crate::prefs::MONO_FONT,
        "icon": "Material Symbols Rounded",
    });
    tokens["gradients"] = prefs.interface_gradients.into();
    bead::ui::theme::set(tokens).expect("Nacre theme tokens match the framework schema");
    let mut style = prefs.text_style();
    style.fixed_size_prefixes.push("Material Symbols".into());
    let _ = bead::ui::text_style::set(style);
}

// Match ui.tint's sRGB channel rounding, then enforce the icon contrast floor.
pub(crate) fn launcher_icon_tint(palette: &bead::render::Palette) -> Option<f64> {
    let base = palette.get("onSurfaceVariant").ok()?;
    let accent = palette.get("primary").ok()?;
    let surface = palette.get("surface").ok()?;
    (0..=7)
        .rev()
        .map(|step| f64::from(step) / 20.)
        .find(|amount| {
            let ink = std::array::from_fn(|i| {
                (f64::from(base[i]) * (1. - amount) + f64::from(accent[i]) * amount).round() as u8
            });
            icon_contrast(ink, surface) >= 3.
        })
}

fn icon_contrast(a: [u8; 4], b: [u8; 4]) -> f64 {
    let luma = |color: [u8; 4]| {
        let linear = color.map(|channel| {
            let c = f64::from(channel) / 255.;
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        });
        0.2126 * linear[0] + 0.7152 * linear[1] + 0.0722 * linear[2]
    };
    let (a, b) = (luma(a), luma(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launcher_glyphs_contrast_in_every_demo_palette() {
        let demos: serde_json::Value =
            serde_json::from_str(include_str!("../../../shell/welcome/demo/palettes.json"))
                .unwrap();
        for scene in demos["scenes"].as_array().unwrap() {
            for mode in ["dark", "light"] {
                for (personality, colors) in scene["palettes"][mode].as_object().unwrap() {
                    let roles = colors
                        .as_object()
                        .unwrap()
                        .iter()
                        .map(|(role, hex)| {
                            let rgb = u32::from_str_radix(
                                hex.as_str().unwrap().trim_start_matches('#'),
                                16,
                            )
                            .unwrap();
                            (
                                role.clone(),
                                [(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 255],
                            )
                        })
                        .collect();
                    let palette = bead::render::Palette(std::sync::Arc::new(roles));
                    let amount = launcher_icon_tint(&palette).expect("safe blend");
                    let base = palette.get("onSurfaceVariant").unwrap();
                    let accent = palette.get("primary").unwrap();
                    let ink = std::array::from_fn(|i| {
                        (f64::from(base[i]) * (1. - amount) + f64::from(accent[i]) * amount).round()
                            as u8
                    });
                    let surface = palette.get("surface").unwrap();
                    assert!(icon_contrast(ink, surface) >= 3., "{mode}/{personality}");
                    assert!(
                        icon_contrast(accent, surface) >= 3.,
                        "selected rail {mode}/{personality}"
                    );
                }
            }
        }
        let palette =
            bead::render::Palette(std::sync::Arc::new(std::collections::BTreeMap::from([
                ("onSurfaceVariant".into(), [120, 120, 120, 255]),
                ("primary".into(), [255, 255, 255, 255]),
                ("surface".into(), [255, 255, 255, 255]),
            ])));
        assert!(launcher_icon_tint(&palette).unwrap() < 0.35);
    }
}
