//! Explicit software-library operations. Imports never execute installation hooks.
use crate::{i18n::tr, paths::Paths, process, Result};
use nix::fcntl::{Flock, FlockArg};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    time::Duration,
};

const LIMIT: u64 = 262_144;
fn bundled() -> Vec<Value> {
    vec![
        json!({"schemaVersion":1,"id":"org.nacre.folders","name":"Quick folders","description":"Open Documents, Downloads and Projects directly from launcher search","version":"1.0.0","publisher":"Nacre","kinds":["launcher"],"actions":[
            {"id":"documents","name":"Open Documents","icon":"folder","command":["xdg-open","{home}/Documents"]},
            {"id":"downloads","name":"Open Downloads","icon":"download","command":["xdg-open","{home}/Downloads"]},
            {"id":"projects","name":"Open Projects","icon":"code","command":["xdg-open","{home}/Projects"]}
        ]}),
        json!({"schemaVersion":1,"id":"org.nacre.reference","name":"Reference shortcuts","description":"Searchable shortcuts to Arch documentation, package discovery and developer references","version":"1.0.0","publisher":"Nacre","kinds":["launcher"],"actions":[
            {"id":"arch-wiki","name":"Open Arch Wiki","icon":"menu_book","command":["xdg-open","https://wiki.archlinux.org/"]},
            {"id":"arch-packages","name":"Browse Arch packages","icon":"apps","command":["xdg-open","https://archlinux.org/packages/"]},
            {"id":"developer-reference","name":"Open developer references","icon":"code","command":["xdg-open","https://developer.mozilla.org/"]}
        ]}),
    ]
}
fn install_builtin(paths: &Paths, id: &str) -> Result<Value> {
    let manifest = bundled().into_iter().find(|e| e["id"] == id).ok_or(tr(
        "helper.software-library.bundled-extension-is-no-longer-available",
        "Bundled extension is no longer available",
    ))?;
    let mut value = config(paths)?;
    let target = directory(paths).join(id);
    if target.exists()
        || value["extensions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["id"] == id)
    {
        return Err(tr(
            "helper.software-library.extension-is-already-installed",
            "Extension is already installed",
        )
        .into());
    }
    fs::create_dir_all(&target)?;
    write(&target.join("nacre.extension.json"), &manifest)?;
    let revision = format!("{:x}", Sha256::digest(serde_json::to_vec(&manifest)?));
    value["extensions"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id":id,"source":"bundled","revision":revision,"enabled":false}));
    if let Err(error) = write(&store(paths), &value) {
        fs::remove_dir_all(target)?;
        return Err(error);
    }
    Ok(
        json!({"message":tr("helper.software-library.extension-installed-enable-it-to-add-its-actions", "Extension installed; enable it to add its actions")}),
    )
}
fn clone_extension(paths: &Paths, id: &str, input: &Value) -> Result<Value> {
    let mut value = config(paths)?;
    existing(&value, id)?;
    let source = directory(paths).join(id);
    let mut manifest = tree(&source)?;
    let new_id = text(input, "id", 96)?;
    identifier(new_id)?;
    if new_id.starts_with("org.nacre.") {
        return Err(tr(
            "helper.software-library.choose-your-own-extension-namespace",
            "Choose your own extension namespace",
        )
        .into());
    }
    let target = directory(paths).join(new_id);
    if target.exists()
        || value["extensions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["id"] == new_id)
    {
        return Err(tr(
            "helper.software-library.the-clone-id-is-already-used",
            "The clone ID is already used",
        )
        .into());
    }
    let temp = tempfile::tempdir_in(directory(paths))?;
    let staged = temp.path().join("clone");
    fs::create_dir(&staged)?;
    for entry in walkdir::WalkDir::new(&source)
        .into_iter()
        .filter_entry(|e| e.file_name() != ".git")
    {
        let entry = entry?;
        let dest = staged.join(entry.path().strip_prefix(&source)?);
        if entry.file_type().is_dir() {
            fs::create_dir_all(dest)?;
        } else if entry.file_type().is_file() {
            fs::copy(entry.path(), dest)?;
        }
    }
    manifest["id"] = json!(new_id);
    manifest["name"] = json!(text(input, "name", 80)?);
    write(&staged.join("nacre.extension.json"), &manifest)?;
    tree(&staged)?;
    fs::rename(&staged, &target)?;
    value["extensions"].as_array_mut().unwrap().push(json!({"id":new_id,"source":"local","revision":format!("{:x}",Sha256::digest(serde_json::to_vec(&manifest)?)),"enabled":false}));
    if let Err(error) = write(&store(paths), &value) {
        fs::remove_dir_all(target)?;
        return Err(error);
    }
    Ok(
        json!({"message":tr("helper.software-library.local-clone-created-disabled", "Local clone created disabled"),"folder":target}),
    )
}
fn theme_remove(paths: &Paths, id: &str) -> Result<Value> {
    if !regex::Regex::new(r"\Afavorite:[0-9a-f]{16}\z")?.is_match(id) {
        return Err(tr(
            "helper.software-library.only-saved-themes-can-be-removed",
            "Only saved themes can be removed",
        )
        .into());
    }
    let preferences = read(&paths.config.join("wallpaper-picker.json"), json!({}))?;
    if preferences["palettePreset"] == id {
        return Err(tr(
            "helper.software-library.apply-another-theme-before-removing-this-one",
            "Apply another theme before removing this one",
        )
        .into());
    }
    let path = paths.config.join("colors.json");
    let lock = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path.with_extension("lock"))?;
    let _guard = Flock::lock(lock, FlockArg::LockExclusive).map_err(|(_, error)| error)?;
    let mut saved = read(&path, Value::Null)?;
    let favorites = saved["favorites"].as_array_mut().ok_or(tr(
        "helper.software-library.invalid-saved-themes-original-file-preserved",
        "Invalid saved themes; original file preserved",
    ))?;
    favorites.retain(|f| f["id"] != id.trim_start_matches("favorite:"));
    write(&path, &saved)?;
    Ok(json!({"message":tr("helper.software-library.saved-theme-removed", "Saved theme removed")}))
}
fn theme_export(paths: &Paths, id: &str, input: &Value) -> Result<Value> {
    let palettes: Value = serde_json::from_str(include_str!(
        "../../../nacre/shell-tools/palette-presets.json"
    ))?;
    let saved = read(&paths.config.join("colors.json"), json!({"favorites":[]}))?;
    let item = if let Some(id) = id.strip_prefix("favorite:") {
        saved["favorites"]
            .as_array()
            .and_then(|a| a.iter().find(|e| e["id"] == id))
            .ok_or(tr(
                "helper.software-library.saved-theme-is-no-longer-available",
                "Saved theme is no longer available",
            ))?
    } else {
        palettes
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["id"] == id)
            .ok_or(tr(
                "helper.software-library.theme-is-no-longer-available",
                "Theme is no longer available",
            ))?
    };
    let data = json!({"schemaVersion":1,"id":"user.exported-theme","name":item["name"],"description":"Exported Nacre palette","version":"1.0.0","kinds":["theme"],"theme":{"modes":item["modes"]}});
    let target = Path::new(text(input, "path", 4096)?);
    if !target.is_absolute() || target.exists() {
        return Err(tr(
            "helper.software-library.choose-a-new-absolute-file-path-for-the-theme-export",
            "Choose a new absolute file path for the theme export",
        )
        .into());
    }
    validate(
        target.parent().ok_or(tr(
            "helper.software-library.missing-theme-folder",
            "Missing theme folder",
        ))?,
        &data,
    )?;
    let mut file = tempfile::NamedTempFile::new_in(target.parent().unwrap())?;
    serde_json::to_writer_pretty(&mut file, &data)?;
    file.as_file().sync_all()?;
    file.persist_noclobber(target)?;
    Ok(
        json!({"message":tr("helper.software-library.theme-exported", "Theme exported"),"path":target}),
    )
}

fn store(paths: &Paths) -> PathBuf {
    paths.config.join("software-library.json")
}
fn directory(paths: &Paths) -> PathBuf {
    paths.share.join("software/extensions")
}

fn read(path: &Path, fallback: Value) -> Result<Value> {
    match fs::metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(fallback),
        Err(e) => Err(e.into()),
        Ok(m) if m.len() > 8_388_608 => Err(tr(
            "helper.software-library.software-library-file-is-too-large",
            "Software library file is too large",
        )
        .into()),
        Ok(_) => Ok(serde_json::from_slice(&fs::read(path)?)?),
    }
}
fn write(path: &Path, data: &Value) -> Result<()> {
    if path
        .file_name()
        .is_some_and(|name| name == "software-library.json")
    {
        validate_config(data)?;
    }
    if path.is_symlink() {
        return Err(tr(
            "helper.software-library.a-managed-library-file-cannot-be-a-symbolic-link",
            "A managed library file cannot be a symbolic link",
        )
        .into());
    }
    let parent = path.parent().ok_or(tr(
        "helper.software-library.missing-library-folder",
        "Missing library folder",
    ))?;
    fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    fs::set_permissions(file.path(), fs::Permissions::from_mode(0o600))?;
    serde_json::to_writer_pretty(&mut file, data)?;
    file.write_all(b"\n")?;
    file.as_file().sync_all()?;
    file.persist(path)?;
    Ok(())
}
fn validate_config(value: &Value) -> Result<()> {
    if value["version"] != 1 || value["extensions"].as_array().is_none_or(|a| a.len() > 128) {
        return Err(tr(
            "helper.software-library.invalid-software-library-original-file-preserved",
            "Invalid software library; original file preserved",
        )
        .into());
    }
    let mut seen = std::collections::HashSet::new();
    for row in value["extensions"].as_array().unwrap() {
        let id = row["id"].as_str().ok_or(tr(
            "helper.software-library.extension-needs-an-id",
            "Extension needs an ID",
        ))?;
        identifier(id)?;
        if !seen.insert(id)
            || !row["enabled"].is_boolean()
            || !row["revision"].is_string()
            || !row["source"].is_string()
        {
            return Err(tr(
                "helper.software-library.invalid-extension-record",
                "Invalid extension record",
            )
            .into());
        }
    }
    Ok(())
}
fn config(paths: &Paths) -> Result<Value> {
    let value = read(&store(paths), json!({"version":1,"extensions":[]}))?;
    validate_config(&value)?;
    Ok(value)
}

fn identifier(id: &str) -> Result<()> {
    if id.len() > 96
        || !regex::Regex::new(r"\A[a-z][a-z0-9-]*(?:\.[a-z][a-z0-9-]*)+\z")?.is_match(id)
    {
        return Err(tr(
            "helper.software-library.extension-ids-use-lowercase-dotted-names",
            "Extension IDs use lowercase dotted names",
        )
        .into());
    }
    Ok(())
}
fn text<'a>(data: &'a Value, field: &str, limit: usize) -> Result<&'a str> {
    let value = data[field].as_str().ok_or(tr(
        "helper.software-library.missing-extension-text",
        "Missing extension text",
    ))?;
    if value.trim().is_empty()
        || value.chars().count() > limit
        || value.chars().any(char::is_control)
    {
        return Err(tr(
            "helper.software-library.invalid-extension-text",
            "Invalid extension text",
        )
        .into());
    }
    Ok(value)
}
fn url(value: &str) -> Result<()> {
    if value.len() > 2048
        || !value.starts_with("https://")
        || value.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return Err(tr(
            "helper.software-library.use-a-public-https-repository-or-website-url",
            "Use a public HTTPS repository or website URL",
        )
        .into());
    }
    let host = value[8..].split('/').next().unwrap_or("");
    if host.is_empty() || host.contains('@') || value.contains(['?', '#']) {
        return Err(tr(
            "helper.software-library.url-credentials-and-extra-url-parameters-are-not-supported",
            "URL credentials and extra URL parameters are not supported",
        )
        .into());
    }
    Ok(())
}
fn asset(root: &Path, relative: &str) -> Result<PathBuf> {
    let name = Path::new(relative);
    if name.is_absolute()
        || name
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return Err(tr(
            "helper.software-library.extension-assets-must-stay-inside-their-folder",
            "Extension assets must stay inside their folder",
        )
        .into());
    }
    let path = root.join(name).canonicalize()?;
    if !path.starts_with(root.canonicalize()?)
        || !path.is_file()
        || fs::metadata(&path)?.len() > 4_194_304
    {
        return Err(tr(
            "helper.software-library.extension-asset-is-missing-too-large-or-outside-its-folder",
            "Extension asset is missing, too large, or outside its folder",
        )
        .into());
    }
    Ok(path)
}
fn validate(root: &Path, data: &Value) -> Result<()> {
    if data["schemaVersion"] != 1 {
        return Err(tr(
            "helper.software-library.unsupported-nacre-extension-format",
            "Unsupported Nacre extension format",
        )
        .into());
    }
    identifier(text(data, "id", 96)?)?;
    text(data, "name", 80)?;
    text(data, "description", 600)?;
    text(data, "version", 40)?;
    if let Some(website) = data["website"].as_str() {
        url(website)?;
    }
    let kinds = data["kinds"].as_array().ok_or(tr(
        "helper.software-library.choose-an-extension-kind",
        "Choose an extension kind",
    ))?;
    if kinds.is_empty()
        || kinds.len() > 3
        || kinds
            .iter()
            .any(|v| !matches!(v.as_str(), Some("launcher" | "theme" | "panel")))
    {
        return Err(tr(
            "helper.software-library.supported-extension-kinds-are-launcher-theme-and-panel",
            "Supported extension kinds are launcher, theme and panel",
        )
        .into());
    }
    if kinds.contains(&json!("launcher")) {
        let actions = data["actions"].as_array().ok_or(tr(
            "helper.software-library.launcher-extensions-need-actions",
            "Launcher extensions need actions",
        ))?;
        if actions.is_empty() || actions.len() > 64 {
            return Err(tr(
                "helper.software-library.use-one-to-64-launcher-actions",
                "Use one to 64 launcher actions",
            )
            .into());
        }
        let mut ids = std::collections::HashSet::new();
        let action_id = regex::Regex::new(r"\A[a-z][a-z0-9-]*\z")?;
        for action in actions {
            let id = text(action, "id", 64)?;
            if !action_id.is_match(id) || !ids.insert(id) {
                return Err(tr(
                    "helper.software-library.actions-need-unique-lowercase-ids",
                    "Actions need unique lowercase IDs",
                )
                .into());
            }
            text(action, "name", 80)?;
            let argv = action["command"].as_array().ok_or(tr(
                "helper.software-library.actions-use-an-executable-and-literal-arguments",
                "Actions use an executable and literal arguments",
            ))?;
            if argv.is_empty()
                || argv.len() > 64
                || argv.iter().any(|v| {
                    v.as_str()
                        .is_none_or(|s| s.is_empty() || s.len() > 4096 || s.contains('\0'))
                })
            {
                return Err(tr(
                    "helper.software-library.invalid-extension-command",
                    "Invalid extension command",
                )
                .into());
            }
        }
    }
    if kinds.contains(&json!("theme")) {
        for mode in ["dark", "light"] {
            let colors = data["theme"]["modes"][mode].as_object().ok_or(tr(
                "helper.software-library.themes-need-dark-and-light-palettes",
                "Themes need dark and light palettes",
            ))?;
            orient_core::palette::validate(colors)?;
        }
    }
    if kinds.contains(&json!("panel")) {
        asset(root, text(data, "entryPoint", 240)?)?;
    }
    if let Some(preview) = data["preview"].as_str() {
        asset(root, preview)?;
    }
    // Reject installation hooks rather than ignoring code someone expects to run.
    if data.get("install").is_some() || data.get("hooks").is_some() {
        return Err(tr(
            "helper.software-library.extensions-cannot-include-installation-hooks",
            "Extensions cannot include installation hooks",
        )
        .into());
    }
    Ok(())
}
fn revision(root: &Path) -> Result<String> {
    git(&[
        "-C",
        root.to_str().ok_or(tr(
            "helper.software-library.invalid-extension-path",
            "Invalid extension path",
        ))?,
        "rev-parse",
        "HEAD",
    ])
}
fn git(args: &[&str]) -> Result<String> {
    let mut command = std::process::Command::new("git");
    command
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "protocol.file.allow=never",
            "-c",
            "protocol.ext.allow=never",
            "-c",
            "submodule.recurse=false",
        ])
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE");
    let output = process::capture(command, None, Duration::from_secs(60))?;
    if !output.status.success() {
        return Err(tr(
            "helper.software-library.repository-operation-failed-original-extension-preserved",
            "Repository operation failed; original extension preserved",
        )
        .into());
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}
fn tree(root: &Path) -> Result<Value> {
    let mut count = 0;
    let mut bytes = 0;
    for entry in walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| e.file_name() != ".git")
    {
        let entry = entry?;
        count += 1;
        if entry.file_type().is_symlink() {
            return Err(tr(
                "helper.software-library.extension-bundles-cannot-contain-symbolic-links",
                "Extension bundles cannot contain symbolic links",
            )
            .into());
        }
        if entry.file_type().is_file() {
            bytes += entry.metadata()?.len();
        }
        if count > 2048 || bytes > 33_554_432 {
            return Err(tr(
                "helper.software-library.extension-bundle-is-too-large",
                "Extension bundle is too large",
            )
            .into());
        }
    }
    if root.join(".gitmodules").exists() {
        return Err(tr(
            "helper.software-library.extension-submodules-are-not-supported",
            "Extension submodules are not supported",
        )
        .into());
    }
    if fs::metadata(root.join("nacre.extension.json"))?.len() > LIMIT {
        return Err(tr(
            "helper.software-library.extension-manifest-is-too-large",
            "Extension manifest is too large",
        )
        .into());
    }
    let data = read(&root.join("nacre.extension.json"), Value::Null)?;
    validate(root, &data)?;
    Ok(data)
}
fn existing<'a>(value: &'a Value, id: &str) -> Result<&'a Value> {
    identifier(id)?;
    value["extensions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["id"] == id)
        .ok_or_else(|| "Extension is no longer installed".into())
}
fn list(paths: &Paths) -> Result<Value> {
    let value = config(paths)?;
    let mut entries = Vec::new();
    for record in value["extensions"].as_array().unwrap() {
        let root = directory(paths).join(record["id"].as_str().unwrap());
        let mut item = match read(&root.join("nacre.extension.json"), Value::Null).and_then(
            |item| {
                validate(&root, &item)?;
                Ok(item)
            },
        ) {
            Ok(v) => v,
            Err(error) => {
                json!({"id":record["id"],"name":record["id"],"description":error.to_string(),"broken":true,"actions":[]})
            }
        };
        item["folder"] = json!(root);
        item["enabled"] = record["enabled"].clone();
        item["revision"] = record["revision"].clone();
        item["source"] = record["source"].clone();
        if let Some(preview) = item["preview"].as_str() {
            if let Ok(path) = asset(&root, preview) {
                item["previewPath"] = json!(path);
            }
        }
        entries.push(item);
    }
    for mut item in bundled() {
        if !entries.iter().any(|entry| entry["id"] == item["id"]) {
            item["available"] = json!(true);
            item["enabled"] = json!(false);
            entries.push(item);
        }
    }
    let mut themes: Value = serde_json::from_str(include_str!(
        "../../../nacre/shell-tools/palette-presets.json"
    ))?;
    let saved = read(&paths.config.join("colors.json"), json!({"favorites":[]}))?;
    if let Some(favorites) = saved["favorites"].as_array() {
        for item in favorites {
            if let Some(id) = item["id"]
                .as_str()
                .filter(|id| id.len() == 16 && id.bytes().all(|b| b.is_ascii_hexdigit()))
            {
                let mut item = item.clone();
                item["id"] = json!(format!("favorite:{id}"));
                item["group"] = json!("saved");
                themes.as_array_mut().unwrap().push(item);
            }
        }
    }
    Ok(json!({"ready":true,"extensions":entries,"themes":themes}))
}
fn import(paths: &Paths, source: &str, update: Option<&str>) -> Result<Value> {
    url(source)?;
    fs::create_dir_all(directory(paths))?;
    let temporary = tempfile::tempdir_in(directory(paths))?;
    let staged = temporary.path().join("checkout");
    git(&[
        "clone",
        "--depth",
        "1",
        "--no-recurse-submodules",
        "--",
        source,
        staged.to_str().ok_or(tr(
            "helper.software-library.invalid-extension-path",
            "Invalid extension path",
        ))?,
    ])?;
    let manifest = tree(&staged)?;
    let id = text(&manifest, "id", 96)?;
    if id.starts_with("org.nacre.") {
        return Err(tr(
            "helper.software-library.the-org-nacre-namespace-is-reserved-for-bundled-extensions",
            "The org.nacre namespace is reserved for bundled extensions",
        )
        .into());
    }
    let rev = revision(&staged)?;
    let mut value = config(paths)?;
    let target = directory(paths).join(id);
    if let Some(expected) = update {
        if expected != id {
            return Err(tr(
                "helper.software-library.the-updated-repository-changed-its-extension-id",
                "The updated repository changed its extension ID",
            )
            .into());
        }
        let old = existing(&value, id)?;
        if !git(&["-C", target.to_str().unwrap(), "status", "--porcelain"])?.is_empty() {
            return Err(
                "Local extension edits preserved; clone or save them before updating".into(),
            );
        }
        if old["revision"] == rev {
            return Ok(
                json!({"message":tr("helper.software-library.extension-is-up-to-date", "Extension is up to date")}),
            );
        }
        stop(paths, old)?;
    } else if value["extensions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["id"] == id)
        || target.exists()
    {
        return Err(tr(
            "helper.software-library.this-extension-is-already-installed",
            "This extension is already installed",
        )
        .into());
    }
    let backup = temporary.path().join("previous");
    if target.exists() {
        fs::rename(&target, &backup)?;
    }
    if let Err(error) = fs::rename(&staged, &target) {
        if backup.exists() {
            fs::rename(backup, &target)?;
        }
        return Err(error.into());
    }
    let record = json!({"id":id,"source":source,"revision":rev,"enabled":false});
    let entries = value["extensions"].as_array_mut().unwrap();
    entries.retain(|e| e["id"] != id);
    entries.push(record);
    if let Err(error) = write(&store(paths), &value) {
        fs::remove_dir_all(&target)?;
        if backup.exists() {
            fs::rename(&backup, &target)?;
        }
        return Err(error);
    }
    Ok(
        json!({"message":tr("helper.software-library.extension-imported-disabled-review-it-before-enabling", "Extension imported disabled; review it before enabling"),"id":id}),
    )
}
fn unit(record: &Value) -> Result<Option<(&str, &str)>> {
    let Some(name) = record["unit"].as_str() else {
        return Ok(None);
    };
    let owner = record["owner"].as_str().ok_or(tr(
        "helper.software-library.missing-extension-process-owner",
        "Missing extension process owner",
    ))?;
    if name != format!("nacre-extension-{owner}.scope")
        || !name.starts_with("nacre-extension-")
        || !name.ends_with(".scope")
        || name.contains('/')
        || owner.len() != 32
        || !owner.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err(tr(
            "helper.software-library.invalid-managed-extension-process",
            "Invalid managed extension process",
        )
        .into());
    }
    Ok(Some((name, owner)))
}
fn stop(_paths: &Paths, record: &Value) -> Result<()> {
    if let Some((name, owner)) = unit(record)? {
        let output = process::run(
            "systemctl",
            &["--user", "show", name, "-p", "Description", "--value"].map(String::from),
            None,
            Duration::from_secs(3),
        )?;
        if String::from_utf8(output.stdout)?.trim() == format!("Nacre extension {owner}") {
            process::checked(
                "systemctl",
                &["--user", "stop", name].map(String::from),
                None,
                Duration::from_secs(5),
            )?;
        }
    }
    Ok(())
}
fn change(paths: &Paths, id: &str, operation: &str) -> Result<Value> {
    let mut value = config(paths)?;
    let record = existing(&value, id)?.clone();
    let root = directory(paths).join(id);
    if operation == "remove" {
        stop(paths, &record)?;
        let temporary = tempfile::tempdir_in(directory(paths))?;
        let previous = temporary.path().join("previous");
        fs::rename(&root, &previous)?;
        value["extensions"]
            .as_array_mut()
            .unwrap()
            .retain(|e| e["id"] != id);
        if let Err(error) = write(&store(paths), &value) {
            fs::rename(previous, root)?;
            return Err(error);
        };
        return Ok(
            json!({"message":tr("helper.software-library.extension-removed", "Extension removed")}),
        );
    }
    if operation == "enable" {
        tree(&root)?;
    } else {
        stop(paths, &record)?;
    }
    let row = value["extensions"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|e| e["id"] == id)
        .unwrap();
    row["enabled"] = json!(operation == "enable");
    write(&store(paths), &value)?;
    Ok(
        json!({"message":if operation=="enable"{tr("helper.software-library.extension-enabled", "Extension enabled")}else{tr("helper.software-library.extension-disabled", "Extension disabled")}}),
    )
}
fn open_extension(paths: &Paths, id: &str, action: Option<&str>) -> Result<Value> {
    let mut value = config(paths)?;
    let record = existing(&value, id)?.clone();
    if record["enabled"] != true {
        return Err(tr(
            "helper.software-library.enable-the-extension-before-opening-it",
            "Enable the extension before opening it",
        )
        .into());
    }
    let root = directory(paths).join(id);
    let manifest = tree(&root)?;
    let mut args = vec![
        "--user".to_string(),
        "--scope".into(),
        "--collect".into(),
        "--quiet".into(),
    ];
    if let Some(action) = action {
        let item = manifest["actions"]
            .as_array()
            .and_then(|a| a.iter().find(|a| a["id"] == action))
            .ok_or(tr(
                "helper.software-library.extension-action-is-no-longer-available",
                "Extension action is no longer available",
            ))?;
        args.extend([
            "--".into(),
            "env".into(),
            "-u".into(),
            "LD_LIBRARY_PATH".into(),
            "-u".into(),
            "QML_IMPORT_PATH".into(),
            "-u".into(),
            "QML2_IMPORT_PATH".into(),
            "-u".into(),
            "QT_PLUGIN_PATH".into(),
            "-u".into(),
            "NACRE_LIB_DIR".into(),
        ]);
        args.extend(item["command"].as_array().unwrap().iter().map(|a| {
            a.as_str()
                .unwrap()
                .replace("{home}", paths.home.to_str().unwrap_or(""))
        }));
    } else {
        let entry = asset(&root, text(&manifest, "entryPoint", 240)?)?;
        stop(paths, &record)?;
        let owner = uuid::Uuid::new_v4().simple().to_string();
        let name = format!("nacre-extension-{owner}.scope");
        args.extend([
            format!("--unit={name}"),
            format!("--property=Description=Nacre extension {owner}"),
            "--".into(),
            "env".into(),
            "-u".into(),
            "LD_LIBRARY_PATH".into(),
            "-u".into(),
            "QML_IMPORT_PATH".into(),
            "-u".into(),
            "QML2_IMPORT_PATH".into(),
            "-u".into(),
            "QT_PLUGIN_PATH".into(),
            "-u".into(),
            "NACRE_LIB_DIR".into(),
            "quickshell".into(),
            "-p".into(),
            entry.to_string_lossy().into_owned(),
        ]);
        let row = value["extensions"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|e| e["id"] == id)
            .unwrap();
        row["owner"] = json!(owner);
        row["unit"] = json!(name);
        write(&store(paths), &value)?;
    }
    process::spawn_detached("systemd-run", &args)?;
    Ok(json!({"message":tr("helper.software-library.extension-opened", "Extension opened")}))
}
fn theme_import(paths: &Paths, input: &str) -> Result<Value> {
    let source = Path::new(input);
    let data = read(source, Value::Null)?;
    let root = source.parent().ok_or(tr(
        "helper.software-library.missing-theme-folder",
        "Missing theme folder",
    ))?;
    validate(root, &data)?;
    if !data["kinds"].as_array().unwrap().contains(&json!("theme")) {
        return Err(tr(
            "helper.software-library.choose-a-nacre-theme-manifest",
            "Choose a Nacre theme manifest",
        )
        .into());
    }
    let mut modes = data["theme"]["modes"].clone();
    for mode in ["dark", "light"] {
        for color in modes[mode].as_object_mut().unwrap().values_mut() {
            *color = json!(color
                .as_str()
                .unwrap()
                .trim_start_matches('#')
                .to_lowercase());
        }
    }
    let id = format!("{:x}", Sha256::digest(serde_json::to_vec(&modes)?))[..16].to_string();
    let path = paths.config.join("colors.json");
    let lock = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path.with_extension("lock"))?;
    let _guard = Flock::lock(lock, FlockArg::LockExclusive).map_err(|(_, error)| error)?;
    let mut saved = read(
        &path,
        json!({"version":1,"favorites":[],"wallpapers":{},"history":[]}),
    )?;
    let favorites = saved["favorites"].as_array_mut().ok_or(tr(
        "helper.software-library.invalid-saved-themes-original-file-preserved",
        "Invalid saved themes; original file preserved",
    ))?;
    if favorites.len() >= 64 && !favorites.iter().any(|f| f["id"] == id) {
        return Err(tr(
            "helper.software-library.saved-theme-limit-reached-remove-a-saved-theme-first",
            "Saved theme limit reached; remove a saved theme first",
        )
        .into());
    }
    if favorites.iter().any(|item| item["id"] == id) {
        return Ok(
            json!({"message":tr("helper.software-library.this-palette-is-already-saved", "This palette is already saved"),"preset":format!("favorite:{id}")}),
        );
    }
    let favorite = json!({"id":id,"name":data["name"],"modes":modes,"palette":{"colours":modes["dark"]},"swatches":[modes["dark"]["primary"],modes["dark"]["secondary"],modes["dark"]["tertiary"]]});
    favorites.retain(|f| f["id"] != id);
    favorites.insert(0, favorite);
    write(&path, &saved)?;
    Ok(
        json!({"message":tr("helper.software-library.theme-imported", "Theme imported"),"preset":format!("favorite:{id}")}),
    )
}
pub fn execute(arguments: &[String], input: Option<Value>) -> Result<Value> {
    let mut paths = Paths::environment()?;
    if let Some(data) = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
    {
        paths.share = data.join("nacre");
    }
    let action = arguments.first().map(String::as_str).unwrap_or("load");
    let id = arguments.get(1).map(String::as_str).unwrap_or("");
    if action == "load" {
        return list(&paths);
    }
    fs::create_dir_all(&paths.config)?;
    let file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(paths.config.join("software-library.lock"))?;
    let _guard = Flock::lock(file, FlockArg::LockExclusive).map_err(|(_, error)| error)?;
    match action {
        "install-builtin" => install_builtin(&paths, id),
        "clone" => clone_extension(&paths, id, &input.unwrap_or(Value::Null)),
        "theme-remove" => theme_remove(&paths, id),
        "theme-export" => theme_export(&paths, id, &input.unwrap_or(Value::Null)),
        "extension-theme" => {
            let data = config(&paths)?;
            existing(&data, id)?;
            theme_import(
                &paths,
                directory(&paths)
                    .join(id)
                    .join("nacre.extension.json")
                    .to_str()
                    .ok_or(tr(
                        "helper.software-library.invalid-theme-path",
                        "Invalid theme path",
                    ))?,
            )
        }
        "import" => import(
            &paths,
            input.as_ref().and_then(|v| v["url"].as_str()).unwrap_or(id),
            None,
        ),
        "update" => {
            let value = config(&paths)?;
            let row = existing(&value, id)?;
            import(&paths, text(row, "source", 2048)?, Some(id))
        }
        "enable" | "disable" | "remove" => change(&paths, id, action),
        "open" => open_extension(
            &paths,
            id,
            input
                .as_ref()
                .and_then(|v| v["action"].as_str())
                .or_else(|| arguments.get(2).map(String::as_str)),
        ),
        "theme-import" => theme_import(
            &paths,
            input
                .as_ref()
                .and_then(|v| v["path"].as_str())
                .unwrap_or(id),
        ),
        _ => Err(tr(
            "helper.software-library.unknown-software-library-operation",
            "Unknown software library operation",
        )
        .into()),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn sandbox() -> Result<(tempfile::TempDir, Paths)> {
        let temp = tempfile::tempdir()?;
        let paths = Paths::new(temp.path());
        fs::create_dir_all(&paths.config)?;
        Ok((temp, paths))
    }
    #[test]
    fn bundle_install_enable_clone_disable_remove_preserves_other_entries() -> Result<()> {
        let (_temp, paths) = sandbox()?;
        let initial = list(&paths)?;
        assert_eq!(initial["extensions"].as_array().unwrap().len(), 2);
        install_builtin(&paths, "org.nacre.folders")?;
        assert_eq!(
            existing(&config(&paths)?, "org.nacre.folders")?["enabled"],
            false
        );
        change(&paths, "org.nacre.folders", "enable")?;
        clone_extension(
            &paths,
            "org.nacre.folders",
            &json!({"id":"user.folders","name":"My folders"}),
        )?;
        assert_eq!(
            existing(&config(&paths)?, "user.folders")?["enabled"],
            false
        );
        let cloned = tree(&directory(&paths).join("user.folders"))?;
        assert_eq!(cloned["name"], "My folders");
        assert_eq!(cloned["actions"].as_array().unwrap().len(), 3);
        change(&paths, "org.nacre.folders", "disable")?;
        change(&paths, "org.nacre.folders", "remove")?;
        assert!(directory(&paths).join("user.folders").exists());
        assert!(!directory(&paths).join("org.nacre.folders").exists());
        assert_eq!(
            existing(&config(&paths)?, "user.folders")?["enabled"],
            false
        );
        Ok(())
    }
    #[test]
    fn invalid_manifests_and_assets_never_admit_hooks_or_escape_folders() -> Result<()> {
        let (temp, _paths) = sandbox()?;
        let mut data = bundled().remove(0);
        validate(temp.path(), &data)?;
        data["hooks"] = json!({"install":"sh run.sh"});
        assert!(validate(temp.path(), &data).is_err());
        data.as_object_mut().unwrap().remove("hooks");
        data["actions"][0]["command"] = json!(["echo", "literal space; $(not evaluated)"]);
        validate(temp.path(), &data)?;
        data["actions"][0]["command"] = json!(["echo", "bad\u{0}value"]);
        assert!(validate(temp.path(), &data).is_err());
        assert!(asset(temp.path(), "../outside.qml").is_err());
        let outside = tempfile::NamedTempFile::new()?;
        std::os::unix::fs::symlink(outside.path(), temp.path().join("escape.qml"))?;
        assert!(asset(temp.path(), "escape.qml").is_err());
        for source in [
            "file:///tmp/extension",
            "https://user:secret@example.org/repo",
            "https://example.org/repo?token=example",
            "--upload-pack=bad",
        ] {
            assert!(url(source).is_err());
        }
        assert!(url("https://example.org/extension.git").is_ok());
        Ok(())
    }
    #[test]
    fn theme_export_import_roundtrip_and_active_remove_guard() -> Result<()> {
        let (temp, paths) = sandbox()?;
        let target = temp.path().join("theme.json");
        let palettes: Value = serde_json::from_str(include_str!(
            "../../../nacre/shell-tools/palette-presets.json"
        ))?;
        let preset = palettes[0]["id"].as_str().unwrap();
        theme_export(&paths, preset, &json!({"path":target}))?;
        let result = theme_import(&paths, target.to_str().unwrap())?;
        let id = result["preset"].as_str().unwrap();
        let data = read(&paths.config.join("colors.json"), Value::Null)?;
        assert_eq!(data["favorites"].as_array().unwrap().len(), 1);
        write(
            &paths.config.join("wallpaper-picker.json"),
            &json!({"palettePreset":id,"otherSetting":true}),
        )?;
        assert!(theme_remove(&paths, id).is_err());
        write(
            &paths.config.join("wallpaper-picker.json"),
            &json!({"palettePreset":"wallpaper","otherSetting":true}),
        )?;
        theme_remove(&paths, id)?;
        assert_eq!(
            read(&paths.config.join("colors.json"), Value::Null)?["favorites"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
        assert!(theme_export(&paths, preset, &json!({"path":target})).is_err());
        Ok(())
    }
    #[test]
    fn malformed_library_is_preserved_and_disabled_extensions_cannot_execute() -> Result<()> {
        let (_temp, paths) = sandbox()?;
        install_builtin(&paths, "org.nacre.folders")?;
        assert!(open_extension(&paths, "org.nacre.folders", Some("documents")).is_err());
        fs::write(store(&paths), b"not json")?;
        assert!(install_builtin(&paths, "org.nacre.reference").is_err());
        assert_eq!(fs::read(store(&paths))?, b"not json");
        assert!(!directory(&paths).join("org.nacre.reference").exists());
        Ok(())
    }
}
