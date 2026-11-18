//! The exported palette (Orient roles plus the lacquer recipe) must stay inside the protocol's
//! wallpaper palette cap with headroom: 138 roles once made `wallpaper.active` unavailable.
use nacre_helper::orient;
use nacre_protocol::wallpaper::{
    ActiveWallpaper, PaletteMode, Presentation, WallpaperFileIdentity, WallpaperPalette,
    PALETTE_ROLES_MAX,
};
use serde_json::json;
use std::fs;

#[test]
fn the_exported_palette_must_pass_the_protocols_active_wallpaper_validation_with_headroom() {
    let folder = tempfile::tempdir().unwrap();
    let input = folder.path().join("image.ppm");
    fs::write(&input, "P3\n2 1\n255\n35 90 170 240 170 45\n").unwrap();
    let poster = folder.path().join("poster.png");
    fs::write(&poster, orient_decoder::artwork_png(&input).unwrap()).unwrap();
    for mode in ["dark", "light"] {
        let options = json!({"mode": mode, "cache_dir": folder.path().join(mode)});
        let palette = orient::from_image(&poster, &options).unwrap();
        let colours: std::collections::BTreeMap<String, String> = palette["colours"]
            .as_object()
            .unwrap()
            .iter()
            .filter_map(|(name, value)| Some((name.clone(), format!("#{}", value.as_str()?))))
            .collect();
        let roles = colours.len();
        let active = ActiveWallpaper {
            revision: 1,
            palette_revision: 1,
            path: "/home/user/wall.png".into(),
            file_identity: WallpaperFileIdentity {
                device: "1".into(),
                inode: "1".into(),
                size: 4096,
                mtime_sec: 1,
                mtime_nsec: 0,
                ctime_sec: 1,
                ctime_nsec: 0,
            },
            presentation: Presentation::Crop,
            palette: WallpaperPalette {
                mode: if mode == "light" {
                    PaletteMode::Light
                } else {
                    PaletteMode::Dark
                },
                colours,
            },
            motion: None,
        };
        active.validate().unwrap_or_else(|e| {
            panic!("{mode}: {roles} roles fail ActiveWallpaper validation (cap {PALETTE_ROLES_MAX}): {e:?}")
        });
        assert!(
            roles + 32 <= PALETTE_ROLES_MAX,
            "{mode}: {roles} roles leave under 32 of headroom below the protocol cap {PALETTE_ROLES_MAX}"
        );
    }
}
