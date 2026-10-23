//! The real `colors catalog|preview` commands on a generated wallpaper in a temp HOME, reshaped
//! into the published N0050 types. Nothing reads the owner's wallpapers or settings.
use nacre_helper::daemon::colors_app::{catalog_reply, preview_reply};
use nacre_protocol::colors::{ColorsCatalog, ColorsPreviewResult};
use serde_json::{json, Value};
use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

fn helper(home: &Path, args: &[&str], input: Option<&str>) -> Value {
    let mut child = Command::new(env!("CARGO_BIN_EXE_nacre-helper"))
        .args(args)
        .env("HOME", home)
        .env("PATH", "")
        .env(
            "NACRE_HELPER_TOOLS",
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../nacre/shell-tools"),
        )
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("XDG_STATE_HOME")
        .env_remove("XDG_CACHE_HOME")
        .env_remove("NACRE_DAEMON_SOCKET")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    if let Some(input) = input {
        stdin.write_all(input.as_bytes()).unwrap();
    }
    drop(stdin);
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn real_helper_output_reshapes_into_the_published_types() {
    let home = tempfile::tempdir().unwrap();
    let library = home.path().join("Pictures/Wallpapers");
    std::fs::create_dir_all(&library).unwrap();
    let image = library.join("lake_shore.png");
    image::RgbImage::from_fn(320, 200, |x, y| {
        if x < 160 {
            image::Rgb([40, 90 + (y / 4) as u8, 170])
        } else {
            image::Rgb([220, 170, (x / 4) as u8])
        }
    })
    .save(&image)
    .unwrap();

    let raw = helper(home.path(), &["colors", "catalog"], None);
    let catalog: ColorsCatalog = serde_json::from_value(
        serde_json::to_value(
            catalog_reply(&raw, &mut |sources| vec![String::new(); sources.len()]).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(catalog.entries.len(), 1);
    assert_eq!(catalog.entries[0].name, "lake shore");
    assert!(!catalog.entries[0].dynamic);
    assert!(raw["entries"][0]["thumbnail"]
        .as_str()
        .is_some_and(|p| p.ends_with(".jpg")));

    let request = json!({"image":image,"mode":"dark","personality":"natural","compare":true});
    let raw = helper(
        home.path(),
        &["colors", "preview"],
        Some(&format!("{request}\n")),
    );
    let result: ColorsPreviewResult = serde_json::from_value(
        serde_json::to_value(preview_reply(&raw, String::new()).unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(result.id.len(), 64);
    assert_eq!(result.name, "lake shore");
    assert_eq!(result.comparisons.len(), 8);
    assert!(result.colours.len() >= 17, "{}", result.colours.len());
    let hex = |text: &String| text.len() == 6 && text.bytes().all(|b| b.is_ascii_hexdigit());
    assert!(result.colours.values().all(hex));
    assert!(result
        .audit
        .simulated
        .keys()
        .eq(["deutan", "protan", "tritan"]));
    assert!(result.audit.simulated["protan"].values().all(hex));
    assert!(!result.audit.text.is_empty());
    assert!(result.image_colors.iter().all(|c| hex(&c.hex)));
    assert!(raw["thumbnail"]
        .as_str()
        .is_some_and(|p| p.ends_with(".jpg")));
    // Without `compare` there are no comparisons.
    let plain = json!({"image":image,"mode":"light"});
    let raw = helper(
        home.path(),
        &["colors", "preview"],
        Some(&format!("{plain}\n")),
    );
    assert!(preview_reply(&raw, String::new())
        .unwrap()
        .comparisons
        .is_empty());
}
