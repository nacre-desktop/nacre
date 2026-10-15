//! Source-session installation copies; no services or owner state are changed.
use crate::{json, Result};
use serde_json::{json as value, Value};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

pub fn validate_profile(input: &Value) -> Result<Value> {
    validate_profile_value(input).map_err(|error| {
        let message = error.to_string();
        let translated = match message.as_str() {
            "Unknown profile fields" => crate::i18n::tr(
                "helper.profile-policy.unknown-profile-fields",
                "Unknown profile fields",
            ),
            "Unsupported profile identity/version" => crate::i18n::tr(
                "helper.profile-policy.unsupported-profile-identity-version",
                "Unsupported profile identity/version",
            ),
            "Workspace count must be 1–10" => crate::i18n::tr(
                "helper.profile-policy.workspace-count-must-be-1-10",
                "Workspace count must be 1–10",
            ),
            "Invalid optional component list" => crate::i18n::tr(
                "helper.profile-policy.invalid-optional-component-list",
                "Invalid optional component list",
            ),
            "Invalid workspace labels/icons" => crate::i18n::tr(
                "helper.profile-policy.invalid-workspace-labels-icons",
                "Invalid workspace labels/icons",
            ),
            "Invalid optional app workspace" => crate::i18n::tr(
                "helper.profile-policy.invalid-optional-app-workspace",
                "Invalid optional app workspace",
            ),
            "Window trash binding preference must be a boolean" => crate::i18n::tr(
                "helper.profile-policy.invalid-window-trash-preference",
                "Window trash binding preference must be a boolean",
            ),
            "Invalid assistant permission preference" => crate::i18n::tr(
                "helper.profile-policy.invalid-assistant-permission-preference",
                "Invalid assistant permission preference",
            ),
            "Invalid profile routes/startup list" => crate::i18n::tr(
                "helper.profile-policy.invalid-profile-routes-startup-list",
                "Invalid profile routes/startup list",
            ),
            "Invalid profile entry" => crate::i18n::tr(
                "helper.profile-policy.invalid-profile-entry",
                "Invalid profile entry",
            ),
            "Invalid profile entry text" => crate::i18n::tr(
                "helper.profile-policy.invalid-profile-entry-text",
                "Invalid profile entry text",
            ),
            "Invalid startup arguments" => crate::i18n::tr(
                "helper.profile-policy.invalid-startup-arguments",
                "Invalid startup arguments",
            ),
            "Invalid startup class list" => crate::i18n::tr(
                "helper.profile-policy.invalid-startup-class-list",
                "Invalid startup class list",
            ),
            _ => return error,
        };
        translated.into()
    })
}
fn validate_profile_value(input: &Value) -> Result<Value> {
    let mut profile = value!({"version":1,"id":"default","components":["configs","shell"],"workspaceCount":6,"workspaceNames":[],"workspaceIcons":[],"startup":[],"routes":[],"brainWorkspace":null,"aiWorkspace":null,"aiFullAccess":false,"windowTrashBindings":false});
    let fields = input.as_object().ok_or("Unknown profile fields")?;
    if fields
        .keys()
        .any(|key| profile.get(key).is_none() && key != "aurReview")
    {
        return Err("Unknown profile fields".into());
    }
    for (key, item) in fields {
        profile[key] = item.clone();
    }
    let id = match profile["id"].as_str() {
        Some(id) => std::borrow::Cow::Borrowed(id),
        None => std::borrow::Cow::Owned(
            profile["id"]
                .as_u64()
                .ok_or("Unsupported profile identity/version")?
                .to_string(),
        ),
    };
    if !(profile["version"].as_f64() == Some(1.0) || profile["version"] == true)
        || id.is_empty()
        || id.len() > 40
        || !id.as_bytes()[0].is_ascii_lowercase() && !id.as_bytes()[0].is_ascii_digit()
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err("Unsupported profile identity/version".into());
    }
    if profile
        .get("aurReview")
        .is_some_and(|v| !matches!(v.as_str(), Some("required" | "native")))
    {
        return Err("Invalid registered AUR review policy".into());
    }
    let count = profile["workspaceCount"]
        .as_u64()
        .filter(|count| (1..=10).contains(count))
        .ok_or("Workspace count must be 1–10")?;
    let components = profile["components"]
        .as_array()
        .ok_or("Invalid optional component list")?;
    let mut seen = std::collections::BTreeSet::new();
    for component in components {
        let name = component
            .as_str()
            .ok_or("Invalid optional component list")?;
        if !["configs", "apps", "shell", "brain", "ai"].contains(&name) || !seen.insert(name) {
            return Err("Invalid optional component list".into());
        }
    }
    for name in ["workspaceNames", "workspaceIcons"] {
        let labels = profile[name]
            .as_array()
            .filter(|labels| labels.len() <= count as usize)
            .ok_or("Invalid workspace labels/icons")?;
        if labels.iter().any(|label| {
            label
                .as_str()
                .is_none_or(|text| text.chars().count() > 100 || text.contains('\0'))
        }) {
            return Err("Invalid workspace labels/icons".into());
        }
    }
    for name in ["brainWorkspace", "aiWorkspace"] {
        if !profile[name].is_null()
            && profile[name]
                .as_u64()
                .is_none_or(|workspace| !(1..=count).contains(&workspace))
        {
            return Err("Invalid optional app workspace".into());
        }
    }
    if !profile["windowTrashBindings"].is_boolean() {
        return Err("Window trash binding preference must be a boolean".into());
    }
    if !profile["aiFullAccess"].is_boolean() {
        return Err("Invalid assistant permission preference".into());
    }
    for name in ["startup", "routes"] {
        let entries = profile[name]
            .as_array()
            .filter(|entries| entries.len() <= 64)
            .ok_or("Invalid profile routes/startup list")?;
        for entry in entries {
            let expected = if name == "startup" {
                vec!["label", "workspace", "argv", "classes"]
            } else {
                vec!["name", "match", "workspace"]
            };
            let fields = entry.as_object().ok_or("Invalid profile entry")?;
            if fields.len() != expected.len()
                || expected.iter().any(|key| !fields.contains_key(*key))
                || entry["workspace"]
                    .as_u64()
                    .is_none_or(|workspace| !(1..=count).contains(&workspace))
            {
                return Err("Invalid profile entry".into());
            }
            let texts = if name == "startup" {
                vec!["label"]
            } else {
                vec!["name", "match"]
            };
            for key in texts {
                if entry[key].as_str().is_none_or(|text| {
                    text.is_empty()
                        || text.chars().count() > 200
                        || text.contains(['\0', '\n', '\r'])
                }) {
                    return Err("Invalid profile entry text".into());
                }
            }
            if name == "startup" {
                let arguments = entry["argv"]
                    .as_array()
                    .filter(|arguments| (1..=32).contains(&arguments.len()))
                    .ok_or("Invalid startup arguments")?;
                if arguments
                    .iter()
                    .any(|argument| argument.as_str().is_none_or(|text| text.contains('\0')))
                {
                    return Err("Invalid startup arguments".into());
                }
                let classes = entry["classes"]
                    .as_array()
                    .ok_or("Invalid startup class list")?;
                if classes
                    .iter()
                    .any(|class| class.as_str().is_none_or(|text| text.chars().count() > 200))
                {
                    return Err("Invalid startup class list".into());
                }
            }
        }
    }
    Ok(profile)
}

pub(crate) fn copied(source: &Path, target: &Path) -> Result<()> {
    fs::create_dir_all(target.parent().ok_or("Invalid staging target")?)?;
    fs::copy(source, target)?;
    fs::set_permissions(target, fs::metadata(source)?.permissions())?;
    let metadata = fs::metadata(source)?;
    fs::File::options().write(true).open(target)?.set_times(
        fs::FileTimes::new()
            .set_accessed(metadata.accessed()?)
            .set_modified(metadata.modified()?),
    )?;
    Ok(())
}

pub fn copy_tree(source: &Path, target: &Path, ignored: &[&str]) -> Result<()> {
    fs::create_dir_all(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        if ignored.iter().any(|name| entry.file_name() == *name) {
            continue;
        }
        let destination = target.join(entry.file_name());
        if entry.path().is_dir() {
            copy_tree(&entry.path(), &destination, ignored)?;
        } else {
            copied(&entry.path(), &destination)?;
        }
    }
    fs::set_permissions(target, fs::metadata(source)?.permissions())?;
    Ok(())
}

pub fn file_map(repo: &Path, profile: &Value) -> Result<BTreeMap<PathBuf, PathBuf>> {
    let mut files = BTreeMap::new();
    fn discover(
        repo: &Path,
        folder: &Path,
        prefix: &Path,
        files: &mut BTreeMap<PathBuf, PathBuf>,
    ) -> Result<()> {
        if !folder.is_dir() {
            return Ok(());
        }
        for entry in fs::read_dir(folder)? {
            let entry = entry?;
            let path = entry.path();
            if entry.file_type()?.is_dir() {
                discover(repo, &path, prefix, files)?;
            } else if path.is_file() {
                let relative = path.strip_prefix(repo)?;
                if relative
                    .components()
                    .any(|component| component.as_os_str() == "__pycache__")
                    || path
                        .extension()
                        .is_some_and(|extension| extension == "pyc" || extension == "pyo")
                    || entry.file_name() == ".qmlls.ini"
                {
                    continue;
                }
                files.insert(prefix.join(relative), path);
            }
        }
        Ok(())
    }
    for name in ["hypr", "kitty", "fastfetch", "fish", "uwsm"] {
        discover(repo, &repo.join(name), Path::new(".config"), &mut files)?;
    }
    for name in [
        "nacre-app",
        "nacre-ai",
        "nacre-settings",
        "nacre-colors",
        "nacre-welcome",
        "orient",
        "nacre-terminal-logo",
        "nacre-terminal",
        "nacre-brain",
        "nacre-brain-maintain",
        "nacre-brain-sync",
        "xdg-open",
    ] {
        files.insert(
            Path::new(".local/bin").join(name),
            repo.join("bin").join(name),
        );
    }
    for name in [
        "org.kde.dolphin.desktop",
        "nacre-ai.desktop",
        "nacre-welcome.desktop",
        "nacre-colors.desktop",
        "nacre-settings.desktop",
        "nacre-terminal.desktop",
        "nacre-brain.desktop",
    ] {
        files.insert(
            Path::new(".local/share/applications").join(name),
            repo.join("nacre/desktop").join(name),
        );
    }
    for kind in ["sync", "check"] {
        for suffix in ["service", "timer"] {
            let name = format!("nacre-brain-{kind}.{suffix}");
            files.insert(
                Path::new(".config/systemd/user").join(&name),
                repo.join("ai/systemd").join(name),
            );
        }
    }
    for name in ["nacre-power-key.service", "nacre-niri-widths.service"] {
        files.insert(
            Path::new(".config/systemd/user").join(name),
            repo.join("nacre/shell-tools").join(name),
        );
    }
    let components = profile["components"]
        .as_array()
        .ok_or("Invalid optional component list")?;
    files.retain(|target, source| {
        let name = target.file_name().unwrap_or_default().to_string_lossy();
        source.exists()
            && (!name.starts_with("nacre-ai") || components.contains(&value!("ai")))
            && (!name.starts_with("nacre-brain") || components.contains(&value!("brain")))
    });
    if repo.join("package.json.installed").exists() {
        files.remove(Path::new(".local/bin/orient"));
        for component in ["core", "ai", "brain"] {
            if component != "core" && !components.contains(&value!(component)) {
                continue;
            }
            let tree = repo.join("deployment").join(component);
            discover(&tree, &tree, Path::new(""), &mut files)?;
        }
    }
    Ok(files)
}

pub fn stage(
    repo: &Path,
    destination: &Path,
    profile: &Value,
    files: &BTreeMap<PathBuf, PathBuf>,
    compiler_directory: &Path,
    helper: &Path,
) -> Result<String> {
    if destination.is_dir() && fs::read_dir(destination)?.next().is_some() {
        return Err("Preparation destination must be empty".into());
    }
    fs::create_dir_all(destination)?;
    let profile = validate_profile(profile)?;
    for (relative, source) in files {
        copied(source, &destination.join(relative))?;
    }
    let data = destination.join(".config/nacre");
    fs::create_dir_all(&data)?;
    fs::write(data.join("profile.json"), json::pretty(&profile)? + "\n")?;
    let contract: Value = serde_json::from_slice(&fs::read(
        compiler_directory.join("compositor-contract.json"),
    )?)?;
    let profile_lua = crate::compositor_config::render_profile(
        &profile,
        "hyprland",
        &Default::default(),
        &contract,
        compiler_directory,
    )?;
    fs::write(
        data.join("profile.lua"),
        "-- Generated by the selected Nacre profile.\n".to_owned()
            + if profile_lua.is_empty() {
                "\n"
            } else {
                &profile_lua
            },
    )?;
    let shell = destination.join(".local/share/nacre/shell");
    copy_tree(
        &repo.join("nacre/shell"),
        &shell.join("source"),
        &["__pycache__"],
    )?;
    copy_tree(
        &repo.join("nacre/shell-tools"),
        &shell.join("tools"),
        &["tests", "__pycache__", "nacre-helper"],
    )?;
    copied(helper, &shell.join("tools/nacre-helper"))?;
    let units = destination.join(".config/systemd/user");
    for name in ["nacre-shell.service", "nacre-niri-widths.service"] {
        copied(
            &repo.join("nacre/shell-tools").join(name),
            &units.join(name),
        )?;
    }
    let components = profile["components"]
        .as_array()
        .ok_or("Invalid optional component list")?;
    if components.contains(&value!("ai")) {
        copied(
            &repo.join("nacre/shell-tools/nacre-sidebar-ai.service"),
            &units.join("nacre-sidebar-ai.service"),
        )?;
    }
    if components.contains(&value!("brain")) {
        copy_tree(
            &repo.join("brain"),
            &destination.join(".local/share/nacre/brain"),
            &["tests", "__pycache__"],
        )?;
        copied(
            &repo.join("brain/brain.service"),
            &units.join("nacre-brain.service"),
        )?;
    }
    fs::write(
        destination.join("installation.json"),
        json::pretty(
            &value!({"version":fs::read_to_string(repo.join("VERSION"))?.trim(),"profile":profile["id"],"components":profile["components"],"prepared":true,"activated":false}),
        )? + "\n",
    )?;
    Ok(format!(
        "Prepared {} installation at {}; no services, packages, devices or accounts changed.\n",
        profile["id"]
            .as_str()
            .ok_or("can only concatenate str (not \"int\") to str")?,
        destination.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    #[test]
    fn frozen_source_installation_file_maps_filter_optional_commands() -> Result<()> {
        let reference: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/niri-session-file-map-reference.json"
        ))?;
        for case in reference["cases"].as_array().ok_or("Missing map cases")? {
            let repo = tempfile::tempdir()?;
            for name in reference["inputs"].as_array().ok_or("Missing map inputs")? {
                let path = repo.path().join(name.as_str().ok_or("Invalid map input")?);
                fs::create_dir_all(path.parent().ok_or("Invalid map path")?)?;
                fs::write(path, "fixture\n")?;
            }
            let files = file_map(repo.path(), &value!({"components":case["components"]}))?;
            let observed = files
                .iter()
                .map(|(target, source)| {
                    Ok((
                        target.to_str().ok_or("Invalid target")?.to_owned(),
                        value!(source
                            .strip_prefix(repo.path())?
                            .to_str()
                            .ok_or("Invalid source")?),
                    ))
                })
                .collect::<Result<serde_json::Map<_, _>>>()?;
            assert_eq!(Value::Object(observed), case["files"]);
        }
        Ok(())
    }
    #[test]
    fn frozen_profile_validation_used_before_any_staging_copies() -> Result<()> {
        let reference: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/niri-session-profile-reference.json"
        ))?;
        for case in reference["cases"]
            .as_array()
            .ok_or("Missing profile cases")?
        {
            match validate_profile(&case["request"]) {
                Ok(profile) => {
                    assert_eq!(profile, case["observed"]["value"], "{}", case["request"])
                }
                Err(error) => assert_eq!(
                    error.to_string(),
                    case["observed"]["error"]["message"].as_str().unwrap(),
                    "{}",
                    case["request"]
                ),
            }
        }
        Ok(())
    }
    #[test]
    fn frozen_pseudo_language_errors_keep_existing_translation_ids() -> Result<()> {
        let reference: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/niri-session-profile-pseudo-reference.json"
        ))?;
        for case in reference["cases"]
            .as_array()
            .ok_or("Missing pseudo cases")?
        {
            let error = validate_profile_value(&case["request"])
                .expect_err("Invalid profile accepted")
                .to_string();
            assert_eq!(crate::i18n::pseudo(&error), case["error"]);
        }
        Ok(())
    }
    #[test]
    fn frozen_profile_identity_type_coercion_keeps_legacy_values() -> Result<()> {
        let reference: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/niri-session-profile-id-reference.json"
        ))?;
        for case in reference["cases"].as_array().ok_or("Missing ID cases")? {
            match validate_profile(&case["request"]) {
                Ok(profile) => assert_eq!(profile, case["observed"]["value"]),
                Err(error) => assert_eq!(
                    error.to_string(),
                    case["observed"]["error"].as_str().unwrap()
                ),
            }
        }
        Ok(())
    }
    #[test]
    fn frozen_staging_copies_modes_profiles_and_optional_components() -> Result<()> {
        let reference: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/niri-session-stage-reference.json"
        ))?;
        let compiler = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../nacre/shell-tools")
            .canonicalize()?;
        for case in reference["cases"]
            .as_array()
            .ok_or("Missing staging cases")?
        {
            let temporary = tempfile::tempdir()?;
            let repo = temporary.path().join("repo");
            let destination = temporary.path().join("destination");
            for (name, input) in reference["inputs"]
                .as_object()
                .ok_or("Missing staging inputs")?
            {
                let path = repo.join(name);
                fs::create_dir_all(path.parent().ok_or("Invalid input path")?)?;
                fs::write(&path, input[0].as_str().ok_or("Invalid input text")?)?;
                fs::set_permissions(
                    path,
                    fs::Permissions::from_mode(input[1].as_u64().ok_or("Invalid mode")? as u32),
                )?;
            }
            let files = case["map"]
                .as_object()
                .ok_or("Missing file map")?
                .iter()
                .map(|(target, source)| {
                    Ok((
                        PathBuf::from(target),
                        repo.join(source.as_str().ok_or("Invalid source path")?),
                    ))
                })
                .collect::<Result<BTreeMap<_, _>>>()?;
            let stdout = stage(
                &repo,
                &destination,
                &case["profile"],
                &files,
                &compiler,
                &repo.join("nacre/shell-tools/nacre-helper"),
            )?;
            assert_eq!(
                stdout.replace(destination.to_str().unwrap(), "$DESTINATION"),
                case["stdout"]
            );
            fn collect(
                root: &Path,
                folder: &Path,
                files: &mut serde_json::Map<String, Value>,
            ) -> Result<()> {
                for entry in fs::read_dir(folder)? {
                    let path = entry?.path();
                    if path.is_dir() {
                        collect(root, &path, files)?;
                    } else {
                        files.insert(path.strip_prefix(root)?.to_str().ok_or("Invalid fixture path")?.into(), value!({"text":fs::read_to_string(&path)?,"mode":fs::metadata(&path)?.permissions().mode() & 0o777}));
                    }
                }
                Ok(())
            }
            let mut observed = serde_json::Map::new();
            collect(&destination, &destination, &mut observed)?;
            assert_eq!(Value::Object(observed), case["files"]);
        }
        Ok(())
    }
}
