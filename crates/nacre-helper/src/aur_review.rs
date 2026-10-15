//! Terminal-first AUR review: fetch/read only until every recipe is approved.
use crate::{files, menu, paths::Paths, process, state, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs,
    io::{self, IsTerminal, Write},
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

pub fn policy(paths: &Paths) -> Result<Value> {
    let profile = state::read_strict(&paths.config.join("profile.json"), json!({}))?;
    let (mode, source) = if let Some(value) = profile.get("aurReview") {
        let mode = value
            .as_str()
            .filter(|s| ["required", "native"].contains(s))
            .ok_or("Invalid registered AUR review policy")?;
        (mode, "registered-profile-setting")
    } else if profile["id"].as_str().is_some_and(|id| id != "default") {
        // Preserve pre-existing custom profile behavior without identifying an account or host.
        ("native", "legacy-custom-profile-default")
    } else {
        ("required", "general-profile-default")
    };
    Ok(
        json!({"mode":mode,"provenance":{"source":source,"store":"profile","key":"aurReview","profile":profile["id"].as_str().unwrap_or("default")}}),
    )
}
pub fn available() -> bool {
    ["git", "makepkg", "pacman", "less"]
        .iter()
        .all(|tool| crate::menu_registry::which(tool).is_some())
}

fn dependency(value: &str) -> Result<String> {
    if value.len() > 240 || value.chars().any(char::is_control) {
        return Err("Invalid package dependency".into());
    }
    let name = value.split(['<', '>', '=']).next().unwrap_or("");
    menu::package_name(name)?;
    if !value
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || b"@._+<>:=-".contains(&b))
    {
        return Err("Invalid versioned dependency".into());
    }
    Ok(name.into())
}
fn clean(text: &str) -> String {
    text.chars()
        .flat_map(|c| {
            if c == '\n'
                || c == '\t'
                || (!c.is_control() && !matches!(c,'\u{202a}'..='\u{202e}'|'\u{2066}'..='\u{2069}'))
            {
                c.to_string().chars().collect::<Vec<_>>()
            } else {
                format!("\\u{{{:x}}}", u32::from(c)).chars().collect()
            }
        })
        .collect()
}
fn git() -> Command {
    let mut cmd = Command::new("git");
    cmd.args([
        "-c",
        "core.hooksPath=/dev/null",
        "-c",
        "core.fsmonitor=false",
        "-c",
        "credential.helper=",
        "-c",
        "protocol.file.allow=never",
        "-c",
        "protocol.ext.allow=never",
    ])
    .env("GIT_CONFIG_NOSYSTEM", "1")
    .env("GIT_CONFIG_SYSTEM", "/dev/null")
    .env("GIT_CONFIG_GLOBAL", "/dev/null")
    .env("GIT_TERMINAL_PROMPT", "0")
    .env_remove("GIT_CONFIG_COUNT");
    for key in [
        "GIT_CONFIG",
        "GIT_CONFIG_PARAMETERS",
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_OBJECT_DIRECTORY",
        "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    ] {
        cmd.env_remove(key);
    }
    cmd
}
fn capture(command: Command) -> Result<std::process::Output> {
    let output = process::capture(command, None, Duration::from_secs(60))?;
    if output.stdout.len() + output.stderr.len() > 8_388_608 {
        return Err("AUR tool output exceeded the review limit".into());
    }
    Ok(output)
}
fn checked(command: Command) -> Result<String> {
    let output = capture(command)?;
    if !output.status.success() {
        return Err(format!(
            "AUR discovery failed: {}",
            clean(&String::from_utf8_lossy(&output.stderr))
        )
        .into());
    }
    Ok(String::from_utf8(output.stdout)?)
}
fn pacman_query(args: &[&str]) -> Result<bool> {
    let mut cmd = Command::new("pacman");
    cmd.args(args);
    let out = capture(cmd)?;
    match out.status.code() {
        Some(0) => Ok(true),
        Some(1) | Some(127) => Ok(false),
        _ => Err("Package database query failed; no install was started".into()),
    }
}

#[derive(Clone, Debug)]
pub struct Recipe {
    pub base: String,
    pub directory: PathBuf,
    pub revision: String,
    pub digest: String,
    pub document: String,
    pub dependencies: Vec<String>,
    pub packages: Vec<String>,
    pub requested: BTreeSet<String>,
}
fn tracked(directory: &Path) -> Result<(String, String)> {
    let mut cmd = git();
    cmd.args(["-C"]).arg(directory).args(["ls-files", "-z"]);
    let out = capture(cmd)?;
    if !out.status.success() {
        return Err("Cannot read AUR checkout".into());
    }
    let mut document = String::new();
    let mut total = 0;
    for (index, name) in out
        .stdout
        .split(|b| *b == 0)
        .filter(|v| !v.is_empty())
        .enumerate()
    {
        if index >= 512 {
            return Err("Too many files in AUR recipe".into());
        }
        let name = std::str::from_utf8(name)?;
        let path = Path::new(name);
        if path.is_absolute()
            || path
                .components()
                .any(|p| !matches!(p, std::path::Component::Normal(_)))
        {
            return Err("Unsafe AUR recipe filename".into());
        }
        let file = directory.join(path);
        let mut prefix = directory.to_path_buf();
        for component in path.components() {
            prefix.push(component.as_os_str());
            if prefix.is_symlink() {
                return Err("Symlinks cannot be reviewed as AUR recipe files".into());
            }
        }
        let meta = fs::symlink_metadata(&file)?;
        if !meta.is_file() || meta.uid() != nix::unistd::getuid().as_raw() || meta.len() > 2_097_152
        {
            return Err("AUR recipe contains an unsupported file or symlink".into());
        }
        let raw = fs::read(&file)?;
        total += raw.len();
        if total > 4_194_304 {
            return Err("AUR recipe exceeds the review size limit".into());
        }
        let hash = format!("{:x}", Sha256::digest(&raw));
        document.push_str(&format!(
            "\n=== {} · SHA-256 {hash} · {} bytes · mode {:03o} ===\n",
            clean(name),
            raw.len(),
            meta.mode() & 0o777
        ));
        if let Some(text) = std::str::from_utf8(&raw).ok().filter(|s| !s.contains('\0')) {
            document.push_str(&clean(text));
            document.push('\n');
        } else {
            if name == "PKGBUILD"
                || name == ".SRCINFO"
                || name.ends_with(".install")
                || name.ends_with(".patch")
                || name.ends_with(".diff")
            {
                return Err("Recipe scripts and patches must be readable text".into());
            }
            document.push_str("Binary asset: digest and size above. It is included unchanged in the approved checkout.\n");
        }
    }
    let digest = format!("{:x}", Sha256::digest(document.as_bytes()));
    Ok((document, digest))
}
fn srcinfo(directory: &Path, base: &str) -> Result<(Vec<String>, Vec<String>)> {
    let text = fs::read_to_string(directory.join(".SRCINFO"))?;
    if text.len() > 262_144 {
        return Err("AUR metadata is too large".into());
    }
    // Arch AUR recipes may omit standard base-devel tools from makedepends.
    // Resolve this official dependency before review; install it only after final consent.
    let mut dependencies = BTreeSet::from(["base-devel".to_owned()]);
    let mut packages = BTreeSet::new();
    let mut valid_base = false;
    for line in text.lines() {
        let Some((key, value)) = line.trim().split_once(" = ") else {
            continue;
        };
        if key == "pkgbase" {
            valid_base = value == base;
        }
        if key == "pkgname" {
            menu::package_name(value)?;
            packages.insert(value.into());
        }
        if ["depends", "makedepends", "checkdepends"]
            .iter()
            .any(|kind| key == *kind || key == format!("{kind}_{}", std::env::consts::ARCH))
        {
            dependency(value)?;
            dependencies.insert(value.into());
        }
    }
    if !valid_base || packages.is_empty() || !directory.join("PKGBUILD").is_file() {
        return Err("AUR checkout metadata does not match its package base".into());
    }
    Ok((
        dependencies.into_iter().collect(),
        packages.into_iter().collect(),
    ))
}
fn fetch(package: &str, root: &Path) -> Result<Recipe> {
    menu::package_name(package)?;
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(20)))
        .build()
        .new_agent();
    let quoted = package
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"_.-".contains(&b) {
                char::from(b).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect::<String>();
    let mut response = agent
        .get(format!(
            "https://aur.archlinux.org/rpc/v5/info?arg%5B%5D={quoted}"
        ))
        .header("User-Agent", "Nacre-AUR-review/1")
        .call()?;
    let raw = response
        .body_mut()
        .with_config()
        .limit(1_048_576)
        .read_to_vec()?;
    let data: Value = serde_json::from_slice(&raw)?;
    let item = data["results"]
        .as_array()
        .and_then(|a| a.iter().find(|p| p["Name"] == package))
        .ok_or("AUR package not found. Choose a concrete package/provider before installing.")?;
    let base = item["PackageBase"]
        .as_str()
        .ok_or("Missing AUR package base")?;
    menu::package_name(base)?;
    let directory = root.join(base);
    if !directory.exists() {
        let mut cmd = git();
        cmd.args([
            "clone",
            "--depth",
            "1",
            "--no-checkout",
            "--single-branch",
            "--",
        ])
        .arg(format!("https://aur.archlinux.org/{base}.git"))
        .arg(&directory);
        checked(cmd)?;
        let mut cmd = git();
        cmd.arg("-C")
            .arg(&directory)
            .args(["checkout", "--detach", "HEAD"]);
        checked(cmd)?;
    }
    let mut cmd = git();
    cmd.arg("-C").arg(&directory).args(["rev-parse", "HEAD"]);
    let revision = checked(cmd)?.trim().to_owned();
    if revision.len() != 40 || !revision.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("Invalid reviewed AUR revision".into());
    }
    let (document, digest) = tracked(&directory)?;
    let (dependencies, packages) = srcinfo(&directory, base)?;
    if !packages.iter().any(|p| p == package) {
        return Err("Requested package is not declared by the AUR recipe".into());
    }
    Ok(Recipe {
        base: base.into(),
        directory,
        revision,
        digest,
        document,
        dependencies,
        packages,
        requested: [package.into()].into_iter().collect(),
    })
}

/// All external effects sit behind this boundary so cancellation can be proved.
pub trait Host {
    fn installed(&mut self, dependency: &str) -> Result<bool>;
    fn official(&mut self, package: &str) -> Result<bool>;
    fn fetch(&mut self, package: &str) -> Result<Recipe>;
    fn review(&mut self, recipe: &Recipe, index: usize, total: usize) -> Result<bool>;
    fn consent(&mut self, recipes: &[Recipe], official: &BTreeSet<String>) -> Result<bool>;
    fn install_official(&mut self, packages: &BTreeSet<String>) -> Result<()>;
    fn build_install(&mut self, recipe: &Recipe, root: bool) -> Result<()>;
}
fn resolve(
    host: &mut impl Host,
    package: &str,
    recipes: &mut Vec<Recipe>,
    official: &mut BTreeSet<String>,
    visiting: &mut BTreeSet<String>,
) -> Result<()> {
    let name = dependency(package)?;
    if host.installed(package)? {
        return Ok(());
    }
    if host.official(&name)? {
        official.insert(package.into());
        return Ok(());
    }
    if recipes.len() + visiting.len() >= 64 {
        return Err("AUR dependency graph exceeds 64 package bases".into());
    }
    if let Some(recipe) = recipes.iter_mut().find(|r| r.packages.contains(&name)) {
        recipe.requested.insert(name);
        return Ok(());
    }
    let recipe = host.fetch(&name)?;
    if !visiting.insert(recipe.base.clone()) {
        return Err("AUR dependency cycle needs manual resolution".into());
    }
    for dep in &recipe.dependencies {
        if !recipe.packages.contains(&dependency(dep)?) {
            resolve(host, dep, recipes, official, visiting)?;
        }
    }
    visiting.remove(&recipe.base);
    recipes.push(recipe);
    Ok(())
}
pub fn workflow(host: &mut impl Host, package: &str) -> Result<bool> {
    let mut recipes = Vec::new();
    let mut official = BTreeSet::new();
    resolve(
        host,
        package,
        &mut recipes,
        &mut official,
        &mut BTreeSet::new(),
    )?;
    for (index, recipe) in recipes.iter().enumerate() {
        if !host.review(recipe, index + 1, recipes.len())? {
            return Ok(false);
        }
    }
    if !host.consent(&recipes, &official)? {
        return Ok(false);
    }
    host.install_official(&official)?;
    for recipe in &recipes {
        host.build_install(recipe, recipe.requested.contains(package))?;
    }
    Ok(true)
}

fn prompt(message: &str, word: &str) -> Result<bool> {
    print!("{message}\nType '{word}' to continue; anything else cancels: ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    Ok(input.trim() == word)
}
fn run_visible(command: &mut Command) -> Result<()> {
    process::restore_child_signals(command);
    let status = command.status()?;
    if !status.success() {
        return Err(format!("Command stopped with {status}").into());
    }
    Ok(())
}
struct Terminal {
    paths: Paths,
    staging: tempfile::TempDir,
    policy: Value,
}
impl Terminal {
    fn record_path(&self, base: &str) -> PathBuf {
        self.paths
            .state
            .join("aur-review/recipes")
            .join(format!("{base}.json"))
    }
}
fn privileged(args: &[String]) -> Result<()> {
    let (tool, prefix) = if crate::menu_registry::which("pkexec").is_some() {
        ("pkexec", vec!["pacman"])
    } else if crate::menu_registry::which("sudo").is_some() {
        ("sudo", vec!["--", "pacman"])
    } else {
        return Err(
            "Administrator authentication requires pkexec or sudo in this visible terminal".into(),
        );
    };
    let mut command = Command::new(tool);
    command.args(prefix).args(args);
    run_visible(&mut command)
}
pub fn review_document(
    recipe: &Recipe,
    policy: &Value,
    record: &Value,
    directory: &Path,
    index: usize,
    total: usize,
) -> Result<String> {
    let mut text=format!("Nacre AUR review {index}/{total}\nPackage base: {}\nExact recipe revision: {}\nPolicy provenance: {}\n\nRead PKGBUILD, .install files, patches and other tracked files below.\nNo build or install has started. Quit the pager, then explicitly approve or cancel.\n",recipe.base,recipe.revision,policy["provenance"]);
    if record["digest"]
        .as_str()
        .is_some_and(|digest| digest != recipe.digest)
    {
        let previous = directory.join("previous-review.txt");
        let current = directory.join("current-review.txt");
        fs::write(
            &previous,
            record["document"]
                .as_str()
                .ok_or("Invalid previous review receipt")?,
        )?;
        fs::write(&current, &recipe.document)?;
        let mut cmd = git();
        cmd.args(["diff", "--no-index", "--no-ext-diff", "--no-textconv", "--"])
            .current_dir(directory)
            .arg("previous-review.txt")
            .arg("current-review.txt");
        let diff = capture(cmd)?;
        if !matches!(diff.status.code(), Some(0 | 1)) {
            return Err("Cannot compare the changed AUR recipe".into());
        }
        text.push_str("\nChanged since your previous reviewed recipe (unified diff):\n");
        text.push_str(&clean(&String::from_utf8(diff.stdout)?));
    } else if record["digest"] == recipe.digest {
        text.push_str("\nUnchanged since your previous review. Review is still required.\n");
    }
    text.push_str("\nComplete current recipe checkout:\n");
    text.push_str(&recipe.document);
    Ok(text)
}

fn pager(directory: &Path, text: &str) -> Result<()> {
    let preview = directory.join("review.txt");
    fs::write(&preview, text)?;
    let mut pager = Command::new("less");
    pager
        .args(["-N", "--"])
        .arg(&preview)
        .env("LESSSECURE", "1")
        .env("LESS", "")
        .env("LESSOPEN", "")
        .env("LESSCLOSE", "")
        .env("LESSKEY", "/dev/null")
        .env("LESSKEYIN", "/dev/null")
        .env("LESSHISTFILE", "/dev/null");
    run_visible(&mut pager)?;
    Ok(())
}

impl Host for Terminal {
    fn installed(&mut self, dep: &str) -> Result<bool> {
        pacman_query(&["-T", "--", dep])
    }
    fn official(&mut self, package: &str) -> Result<bool> {
        pacman_query(&["-Si", "--", package])
    }
    fn fetch(&mut self, package: &str) -> Result<Recipe> {
        fetch(package, self.staging.path())
    }
    fn review(&mut self, recipe: &Recipe, index: usize, total: usize) -> Result<bool> {
        let record = state::read_strict(&self.record_path(&recipe.base), json!({}))?;
        let text = review_document(
            recipe,
            &self.policy,
            &record,
            self.staging.path(),
            index,
            total,
        )?;
        pager(self.staging.path(), &text)?;
        if !prompt(
            &format!("Approve {} at {}?", recipe.base, recipe.revision),
            "reviewed",
        )? {
            return Ok(false);
        }
        state::write(
            &self.record_path(&recipe.base),
            &json!({"version":1,"base":recipe.base,"revision":recipe.revision,"digest":recipe.digest,"document":recipe.document,"reviewedAt":chrono::Utc::now().to_rfc3339(),"provenance":self.policy}),
        )?;
        Ok(true)
    }
    fn consent(&mut self, recipes: &[Recipe], official: &BTreeSet<String>) -> Result<bool> {
        println!(
            "\nReviewed AUR recipes: {}\nRepository dependencies: {}",
            recipes
                .iter()
                .map(|r| r.base.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            official.iter().cloned().collect::<Vec<_>>().join(", ")
        );
        prompt(
            "Build the reviewed recipes and open administrator package-install confirmations?",
            "install",
        )
    }
    fn install_official(&mut self, packages: &BTreeSet<String>) -> Result<()> {
        // Keep the install reason of packages already present; new dependencies are marked as dependencies.
        for existing in [false, true] {
            let selected = packages
                .iter()
                .filter_map(|dep| {
                    match dependency(dep).and_then(|name| pacman_query(&["-Qq", "--", &name])) {
                        Ok(found) if found == existing => Some(Ok(dep.clone())),
                        Ok(_) => None,
                        Err(e) => Some(Err(e)),
                    }
                })
                .collect::<Result<Vec<_>>>()?;
            if !selected.is_empty() {
                let mut args = vec!["-S".into(), "--needed".into()];
                if !existing {
                    args.push("--asdeps".into())
                }
                args.push("--".into());
                args.extend(selected);
                privileged(&args)?;
            }
        }
        Ok(())
    }
    fn build_install(&mut self, recipe: &Recipe, root: bool) -> Result<()> {
        let (_, digest) = tracked(&recipe.directory)?;
        let mut rev = git();
        rev.arg("-C")
            .arg(&recipe.directory)
            .args(["rev-parse", "HEAD"]);
        if digest != recipe.digest || checked(rev)?.trim() != recipe.revision {
            return Err(
                "AUR recipe changed after review; nothing from that checkout was built".into(),
            );
        }
        let destination = recipe.directory.join(".nacre-packages");
        fs::create_dir(&destination)?;
        let mut build = Command::new("nice");
        build
            .args(["-n", "10", "makepkg", "--clean", "--check"])
            .current_dir(&recipe.directory)
            .env("PKGDEST", &destination)
            .env("MAKEFLAGS", "-j2")
            .env("CARGO_BUILD_JOBS", "2")
            .env("CMAKE_BUILD_PARALLEL_LEVEL", "2");
        run_visible(&mut build)?;
        // Inspect produced archives with pacman; never source PKGBUILD again to list files.
        if destination.is_symlink() {
            return Err("Package output directory changed during build".into());
        }
        let mut selected = Vec::new();
        let mut found = BTreeSet::new();
        for entry in fs::read_dir(&destination)? {
            let entry = entry?;
            let file = entry.path();
            if !entry.file_name().to_string_lossy().contains(".pkg.tar.")
                || entry.file_name().to_string_lossy().ends_with(".sig")
            {
                continue;
            }
            let canonical = file.canonicalize()?;
            if file.is_symlink()
                || !canonical.starts_with(destination.canonicalize()?)
                || !canonical.is_file()
            {
                return Err("Built package is outside the owned package directory".into());
            }
            let mut query = Command::new("pacman");
            query.args(["-Qp", "--quiet", "--"]).arg(&canonical);
            let package = checked(query)?.trim().to_owned();
            menu::package_name(&package)?;
            if !recipe.requested.contains(&package) {
                continue;
            }
            if !found.insert(package.clone()) {
                return Err("Multiple built archives provide the same package".into());
            }
            let as_dependency = !root && !pacman_query(&["-Qq", "--", &package])?;
            selected.push((canonical, as_dependency));
        }
        if found != recipe.requested {
            return Err("The build did not produce every requested package".into());
        }
        for as_dependency in [false, true] {
            let packages = selected
                .iter()
                .filter(|(_, reason)| *reason == as_dependency)
                .map(|(file, _)| file.to_string_lossy().into_owned())
                .collect::<Vec<_>>();
            if packages.is_empty() {
                continue;
            }
            let mut args = vec!["-U".into(), "--needed".into()];
            if as_dependency {
                args.push("--asdeps".into());
            }
            args.push("--".into());
            args.extend(packages);
            privileged(&args)?;
        }
        Ok(())
    }
}
/// Inspect a local checkout in the same secure pager, without approving or installing it.
pub fn inspect(directory: &Path) -> Result<i32> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err("AUR review requires a visible terminal".into());
    }
    let paths = Paths::environment()?;
    let text = fs::read_to_string(directory.join(".SRCINFO"))?;
    let base = text
        .lines()
        .find_map(|line| line.trim().strip_prefix("pkgbase = "))
        .ok_or("Missing package base")?;
    menu::package_name(base)?;
    let (dependencies, packages) = srcinfo(directory, base)?;
    let (document, digest) = tracked(directory)?;
    let mut command = git();
    command.arg("-C").arg(directory).args(["rev-parse", "HEAD"]);
    let revision = checked(command)?.trim().to_owned();
    let recipe = Recipe {
        base: base.into(),
        directory: directory.into(),
        revision,
        digest,
        document,
        dependencies,
        packages,
        requested: BTreeSet::new(),
    };
    let record = state::read_strict(
        &paths
            .state
            .join("aur-review/recipes")
            .join(format!("{base}.json")),
        json!({}),
    )?;
    let staging = tempfile::tempdir()?;
    pager(
        staging.path(),
        &review_document(&recipe, &policy(&paths)?, &record, staging.path(), 1, 1)?,
    )?;
    println!("Preview closed. No build, package install or review approval was performed.");
    Ok(0)
}
/// New Software front ends use policy dispatch; the frozen native CLI remains compatible.
pub fn package_install(package: &str) -> Result<i32> {
    use std::os::unix::process::ExitStatusExt;
    menu::package_name(package)?;
    if policy(&Paths::environment()?)?["mode"] == "required" {
        return install(package);
    }
    let args = crate::menu_registry::package_command("install", package)?;
    let mut command = Command::new(&args[0]);
    command.args(&args[1..]);
    process::restore_child_signals(&mut command);
    let status = command.status()?;
    Ok(status
        .code()
        .unwrap_or_else(|| -status.signal().unwrap_or(1)))
}
pub fn install(package: &str) -> Result<i32> {
    menu::package_name(package)?;
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err("AUR review requires a visible terminal".into());
    }
    let paths = Paths::environment()?;
    let policy = policy(&paths)?;
    if policy["mode"] != "required" {
        return Err("This profile uses its native package-manager flow".into());
    }
    if pacman_query(&["-Si", "--", package])? {
        println!("Official repository package: ordinary package-manager confirmation applies.");
        privileged(&["-S".into(), "--needed".into(), "--".into(), package.into()])?;
        return Ok(0);
    }
    if !available() {
        return Err("AUR review requires git, less, pacman and makepkg (base-devel)".into());
    }
    fs::create_dir_all(paths.cache.join("aur-review"))?;
    let _guard = files::lock(&paths.state.join("aur-review/operation.lock"))?;
    let staging = tempfile::Builder::new()
        .prefix("review-")
        .tempdir_in(paths.cache.join("aur-review"))?;
    let mut host = Terminal {
        paths,
        staging,
        policy,
    };
    if workflow(&mut host, package)? {
        Ok(0)
    } else {
        println!("Cancelled. No build or package installation was started.");
        Ok(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    fn recipe(name: &str, deps: &[&str]) -> Recipe {
        Recipe {
            base: name.into(),
            directory: PathBuf::new(),
            revision: "a".repeat(40),
            digest: "b".repeat(64),
            document: "PKGBUILD fixture".into(),
            dependencies: deps.iter().map(|s| (*s).into()).collect(),
            packages: vec![name.into()],
            requested: [name.into()].into_iter().collect(),
        }
    }
    #[derive(Default)]
    struct Fixture {
        recipes: BTreeMap<String, Recipe>,
        events: Vec<String>,
        reject: usize,
        consent: bool,
        official: BTreeSet<String>,
        installed: BTreeSet<String>,
    }
    impl Host for Fixture {
        fn installed(&mut self, value: &str) -> Result<bool> {
            Ok(self.installed.contains(value))
        }
        fn official(&mut self, value: &str) -> Result<bool> {
            Ok(self.official.contains(value))
        }
        fn fetch(&mut self, value: &str) -> Result<Recipe> {
            self.events.push(format!("fetch:{value}"));
            self.recipes
                .get(value)
                .cloned()
                .ok_or_else(|| "Missing fixture recipe".into())
        }
        fn review(&mut self, r: &Recipe, index: usize, total: usize) -> Result<bool> {
            self.events
                .push(format!("review:{}:{index}/{total}", r.base));
            Ok(self.reject != index)
        }
        fn consent(&mut self, _: &[Recipe], _: &BTreeSet<String>) -> Result<bool> {
            self.events.push("consent".into());
            Ok(self.consent)
        }
        fn install_official(&mut self, packages: &BTreeSet<String>) -> Result<()> {
            self.events.push(format!("install:{packages:?}"));
            Ok(())
        }
        fn build_install(&mut self, r: &Recipe, root: bool) -> Result<()> {
            self.events.push(format!("build:{}:{root}", r.base));
            Ok(())
        }
    }
    fn fixture() -> Fixture {
        Fixture {
            recipes: [
                (
                    "editor".into(),
                    recipe("editor", &["library", "python>=3.10", "libc", "base-devel"]),
                ),
                ("library".into(), recipe("library", &[])),
            ]
            .into_iter()
            .collect(),
            official: ["python".into(), "base-devel".into()].into_iter().collect(),
            installed: ["libc".into()].into_iter().collect(),
            consent: true,
            ..Default::default()
        }
    }
    #[test]
    fn cancelling_any_recipe_or_final_consent_has_no_build_or_install_effects() -> Result<()> {
        for reject in [1, 2, 0] {
            let mut host = fixture();
            host.reject = reject;
            host.consent = reject != 0;
            assert!(!workflow(&mut host, "editor")?);
            assert!(!host
                .events
                .iter()
                .any(|event| event.starts_with("build:") || event.starts_with("install:")));
        }
        Ok(())
    }
    #[test]
    fn every_dependency_is_reviewed_before_any_build_or_privileged_install() -> Result<()> {
        let mut host = fixture();
        assert!(workflow(&mut host, "editor")?);
        assert_eq!(
            host.events,
            vec![
                "fetch:editor",
                "fetch:library",
                "review:library:1/2",
                "review:editor:2/2",
                "consent",
                "install:{\"base-devel\", \"python>=3.10\"}",
                "build:library:false",
                "build:editor:true"
            ]
        );
        Ok(())
    }
    #[test]
    fn cycles_and_invalid_dependencies_stop_without_effects() -> Result<()> {
        let mut host = fixture();
        host.recipes
            .get_mut("library")
            .unwrap()
            .dependencies
            .push("editor".into());
        assert!(workflow(&mut host, "editor").is_err());
        assert!(!host.events.iter().any(|event| event.starts_with("review:")
            || event.starts_with("build:")
            || event.starts_with("install:")));
        assert!(dependency("editor; touch anything").is_err());
        assert!(dependency("--noconfirm").is_err());
        assert_eq!(dependency("libfoo>=1:2.0-1")?, "libfoo");
        Ok(())
    }
    #[test]
    fn registered_policy_reports_provenance_and_retains_any_legacy_custom_profile() -> Result<()> {
        let root = tempfile::tempdir()?;
        let paths = Paths::new(root.path());
        assert_eq!(policy(&paths)?["mode"], "required");
        for (profile, expected, source) in [
            (
                json!({"id":"default"}),
                "required",
                "general-profile-default",
            ),
            (
                json!({"id":"studio-profile"}),
                "native",
                "legacy-custom-profile-default",
            ),
            (
                json!({"id":"studio-profile","aurReview":"required"}),
                "required",
                "registered-profile-setting",
            ),
            (
                json!({"id":"default","aurReview":"native"}),
                "native",
                "registered-profile-setting",
            ),
        ] {
            state::write(&paths.config.join("profile.json"), &profile)?;
            let actual = policy(&paths)?;
            assert_eq!(actual["mode"], expected);
            assert_eq!(actual["provenance"]["source"], source);
            assert_eq!(actual["provenance"]["store"], "profile");
        }
        state::write(
            &paths.config.join("profile.json"),
            &json!({"aurReview":"invalid"}),
        )?;
        assert!(policy(&paths).is_err());
        Ok(())
    }
    #[test]
    fn changed_recipe_review_contains_the_previous_diff_and_complete_new_recipe() -> Result<()> {
        let root = tempfile::tempdir()?;
        let mut recipe = recipe("editor", &[]);
        recipe.document =
            "=== PKGBUILD ===\nsource=(new-source)\n=== fix.patch ===\nnew patch\n".into();
        let record =
            json!({"digest":"different","document":"=== PKGBUILD ===\nsource=(old-source)\n"});
        let text = review_document(
            &recipe,
            &json!({"provenance":{"source":"general-profile-default"}}),
            &record,
            root.path(),
            1,
            2,
        )?;
        assert!(text.contains("Changed since your previous reviewed recipe"));
        assert!(text.contains("-source=(old-source)"));
        assert!(text.contains("+source=(new-source)"));
        assert!(text.contains("Complete current recipe checkout"));
        assert!(text.contains("new patch"));
        Ok(())
    }
    #[test]
    fn recipe_preview_reads_all_tracked_scripts_and_patches_without_executing_them() -> Result<()> {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir()?;
        let directory = root.path();
        let mut init = git();
        init.args(["init", "--quiet"]).arg(directory);
        checked(init)?;
        for (name,body) in [("PKGBUILD","touch should-never-exist\npkgname=fixture\n"),(".SRCINFO","pkgbase = fixture\n\tpkgname = fixture\n\tdepends = dep>=1\n\tmakedepends_x86_64 = compiler\n"),("post.install","touch should-never-exist\n"),("fix.patch","--- before\n+++ after\n")] {fs::write(directory.join(name),body)?;}
        let mut add = git();
        add.arg("-C").arg(directory).args(["add", "--", "."]);
        checked(add)?;
        let (document, digest) = tracked(directory)?;
        assert!(document.contains("post.install"));
        assert!(document.contains("fix.patch"));
        assert!(!directory.join("should-never-exist").exists());
        let (dependencies, packages) = srcinfo(directory, "fixture")?;
        assert!(dependencies.contains(&"base-devel".to_owned()));
        assert_eq!(packages, vec!["fixture"]);
        fs::write(directory.join("fix.patch"), "changed patch\n")?;
        assert_ne!(tracked(directory)?.1, digest);
        fs::remove_file(directory.join("post.install"))?;
        symlink(directory.join("PKGBUILD"), directory.join("post.install"))?;
        assert!(tracked(directory).is_err());
        assert!(clean("safe\x1b[2J\u{202e}hidden").contains("\\u{1b}"));
        Ok(())
    }
}
