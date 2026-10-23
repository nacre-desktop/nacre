//! N0050: the Colors app contracts (`colors.catalog|preview|apply|export|targets|set_target`).
//!
//! Every method runs what the QML Colors app ran: the daemon's own executable with the helper
//! commands `colors catalog|preview|apply|export` and `app-theme` (fixed argv; caller text only
//! reaches a child as a JSON line on stdin, validated by the engine). The helper writes the
//! `colors` store through the daemon's provider route, so none of this runs in the daemon
//! process. Replies reshape the helper's output into the published types, and the wallpaper
//! images become small PNGs in two private cache folders (`ImageCache`, symlink-free, bounded).
//!
//! Previews run one at a time and the newest request wins: a newer call cancels the running or
//! waiting older one (its child is killed; the older call fails `busy`). Actions (apply, export,
//! set_target) run one at a time and are never cancelled by a deadline: the child finishes or is
//! stopped by its own limit.
use super::{
    hub::{failure, Hub, Shared},
    notifications::ImageCache,
};
use crate::{paths::Paths, Result};
use nacre_protocol::{colors::*, ErrorCode, Failure, Method};
use serde_json::{json, Map, Value};
use std::{
    cell::Cell,
    collections::BTreeMap,
    path::{Component, Path, PathBuf},
    process::Stdio,
    rc::Rc,
    sync::{Mutex, PoisonError},
    time::Duration,
};

const THUMB_SIDE: u32 = 256;
const THUMB_BYTES: u64 = 256 * 1024;
const POSTER_SIDES: [u32; 3] = [1280, 960, 640];
const POSTER_BYTES: u64 = 2 * 1024 * 1024;
const SOURCE_BYTES: usize = 32 * 1024 * 1024;
const OUTPUT_LIMIT: usize = 8 * 1024 * 1024;
const REPLY_LIMIT: usize = 900_000;
const TARGET_FILE_LIMIT: u64 = 256 * 1024;
const MAX_TARGETS: usize = 64;
/// Each child stops before the method deadline (10 s catalog, 60 s the others).
const CATALOG_LIMIT: Duration = Duration::from_secs(9);
const WORK_LIMIT: Duration = Duration::from_secs(55);

#[derive(Clone)]
pub struct Env {
    /// The daemon's own executable, which hosts the helper commands.
    pub engine: PathBuf,
    pub paths: Paths,
    /// Run children at nice 10 like the QML did.
    pub nice: bool,
}
impl Env {
    fn production() -> Result<Self> {
        Ok(Self {
            engine: std::env::current_exe()?,
            paths: Paths::environment()?,
            nice: true,
        })
    }
    fn thumbs(&self) -> PathBuf {
        self.paths.cache.join("shell/colors-thumbs")
    }
    fn posters(&self) -> PathBuf {
        self.paths.cache.join("shell/colors-preview")
    }
}
thread_local! {
    static OVERRIDE: std::cell::RefCell<Option<Env>> = const { std::cell::RefCell::new(None) };
    /// Bumped by every preview call; an older call sees it change and gives way.
    static GENERATION: tokio::sync::watch::Sender<u64> = tokio::sync::watch::channel(0).0;
    static PREVIEW_LANE: Rc<tokio::sync::Mutex<()>> = Rc::new(tokio::sync::Mutex::new(()));
    static ACTION_BUSY: Rc<Cell<bool>> = Rc::new(Cell::new(false));
}
#[cfg(test)]
pub(crate) fn set_env(env: Option<Env>) {
    OVERRIDE.with(|slot| *slot.borrow_mut() = env);
}
fn env() -> std::result::Result<Env, Failure> {
    if let Some(env) = OVERRIDE.with(|slot| slot.borrow().clone()) {
        return Ok(env);
    }
    Env::production().map_err(|_| unavailable("Colors is unavailable"))
}

fn invalid() -> Failure {
    failure(ErrorCode::Validation, "Invalid Colors request", false).dispatch_state(false)
}
fn unavailable(message: &str) -> Failure {
    failure(ErrorCode::Unavailable, message, true).dispatch_state(false)
}
fn io(message: &str) -> Failure {
    failure(ErrorCode::Io, message, true).dispatch_state(true)
}

// ------------------------------------------------------------- registry

pub fn handles(method: &str) -> bool {
    matches!(
        method,
        "colors.catalog"
            | "colors.preview"
            | "colors.apply"
            | "colors.export"
            | "colors.targets"
            | "colors.set_target"
    )
}
pub fn available_methods(hub: &Hub) -> BTreeMap<String, Method> {
    if hub.safe || hub.settings.is_none() {
        BTreeMap::new()
    } else {
        methods()
    }
}

// ------------------------------------------------------------- children

struct Output {
    code: i32,
    stdout: Vec<u8>,
    stderr: String,
}
/// Run a helper command of the daemon's executable. Dropping the future kills the child.
async fn engine(
    env: &Env,
    args: &[&str],
    input: Option<String>,
    limit: Duration,
) -> std::result::Result<Output, Failure> {
    use tokio::io::AsyncWriteExt;
    let mut command = if env.nice {
        let mut command = tokio::process::Command::new("nice");
        command.args(["-n", "10"]).arg(&env.engine);
        command
    } else {
        tokio::process::Command::new(&env.engine)
    };
    command
        .args(args)
        // The helper must not talk to the daemon's service-manager socket.
        .env_remove("NOTIFY_SOCKET")
        // Apply's presentation stream is for the QML shell; the topics carry the result.
        .env("NACRE_PRESENTATION_STREAM", "0")
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command
        .spawn()
        .map_err(|_| io("Colors engine did not start"))?;
    if let (Some(input), Some(mut stdin)) = (input, child.stdin.take()) {
        // A child that exits early closes the pipe; its own status is reported below.
        let _ = stdin.write_all(input.as_bytes()).await;
    }
    let output = tokio::time::timeout(limit, child.wait_with_output())
        .await
        .map_err(|_| failure(ErrorCode::Timeout, "Colors engine took too long", false))?
        .map_err(|_| io("Colors engine failed"))?;
    if output.stdout.len() > OUTPUT_LIMIT {
        return Err(failure(
            ErrorCode::TooLarge,
            "Colors reply is too large",
            false,
        ));
    }
    Ok(Output {
        code: output.status.code().unwrap_or(1),
        stdout: output.stdout,
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}
/// The engine's last JSON line.
fn last_json(output: &Output) -> Option<Value> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .rev()
        .find_map(|line| serde_json::from_str::<Value>(line).ok())
}
/// A refusal the user can read: the engine's first error line, at most 240 characters.
fn refusal(output: &Output) -> Failure {
    let text: String = output
        .stderr
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("Colors request failed")
        .chars()
        .filter(|c| !c.is_control())
        .take(240)
        .collect();
    failure(ErrorCode::Validation, &text, false).dispatch_state(false)
}
fn engine_value(output: &Output) -> std::result::Result<Value, Failure> {
    if output.code != 0 {
        return Err(refusal(output));
    }
    last_json(output).ok_or_else(|| io("Colors engine returned no result"))
}

// ------------------------------------------------------------- images

fn inside(root: &Path, path: &str) -> bool {
    let path = Path::new(path);
    path.is_absolute()
        && path.starts_with(root)
        && !path
            .components()
            .any(|part| matches!(part, Component::ParentDir))
}
/// A PNG of `source` (a file of the user's cache) in the private cache `root`, or `None`.
fn make_png(
    cache: &ImageCache,
    allowed: &Path,
    source: &str,
    sides: &[u32],
    limit: u64,
) -> Option<String> {
    if !inside(allowed, source) {
        return None;
    }
    sides.iter().find_map(|side| {
        let path = cache
            .resolve_thumbnail(Path::new(source), SOURCE_BYTES, *side)
            .ok()?;
        (std::fs::metadata(&path).ok()?.len() <= limit).then_some(path)
    })
}
/// Convert `sources` and keep only the results; every other file of the folder is removed.
fn pngs(root: PathBuf, allowed: &Path, sources: &[&str], sides: &[u32], limit: u64) -> Vec<String> {
    static GATE: Mutex<()> = Mutex::new(());
    let _gate = GATE.lock().unwrap_or_else(PoisonError::into_inner);
    let Ok(cache) = ImageCache::open(root) else {
        return vec![String::new(); sources.len()];
    };
    let made: Vec<String> = sources
        .iter()
        .map(|source| make_png(&cache, allowed, source, sides, limit).unwrap_or_default())
        .collect();
    let live: Vec<String> = made.iter().filter(|p| !p.is_empty()).cloned().collect();
    let _ = cache.retain(&live);
    made
}

// ------------------------------------------------------------- reshaping

fn text(value: &Value) -> Option<String> {
    value.as_str().map(str::to_owned)
}
fn roles(value: &Value) -> Option<BTreeMap<String, String>> {
    value
        .as_object()?
        .iter()
        .map(|(role, colour)| Some((role.clone(), text(colour)?)))
        .collect()
}
/// The helper's `preview_result` with the palette flattened (`thumbnail` is already a PNG).
pub fn preview_reply(raw: &Value, thumbnail: String) -> Option<ColorsPreviewResult> {
    let audit = &raw["palette"]["accessibility"];
    Some(ColorsPreviewResult {
        id: text(&raw["id"])?,
        request: raw["request"].as_object()?.clone(),
        name: text(&raw["name"])?,
        image: text(&raw["image"])?,
        thumbnail,
        frames: raw["frames"]
            .as_u64()
            .map_or(1, |n| n.clamp(1, u64::from(u32::MAX)) as u32),
        favorite: raw["favorite"].as_bool()?,
        other_coverage: raw["otherCoverage"].as_f64()?,
        image_colors: raw["imageColors"]
            .as_array()?
            .iter()
            .map(|c| {
                Some(ImageColor {
                    hex: text(&c["hex"])?,
                    coverage: c["coverage"].as_f64()?,
                })
            })
            .collect::<Option<_>>()?,
        colours: roles(&raw["palette"]["colours"])?,
        audit: Audit {
            text_passes: audit["textPasses"].as_bool()?,
            minimum_text_contrast: audit["minimumTextContrast"].as_f64()?,
            warnings: audit["warnings"]
                .as_array()?
                .iter()
                .filter_map(text)
                .collect(),
            text: audit["text"]
                .as_array()?
                .iter()
                .map(|row| {
                    Some(AuditText {
                        foreground: text(&row["foreground"])?,
                        background: text(&row["background"])?,
                        ratio: row["ratio"].as_f64()?,
                    })
                })
                .collect::<Option<_>>()?,
            simulated: audit["simulated"]
                .as_object()?
                .iter()
                .map(|(vision, map)| {
                    // The engine names the three kinds in full; the contract uses the short form.
                    let short = match vision.as_str() {
                        "protanopia" => "protan",
                        "deuteranopia" => "deutan",
                        "tritanopia" => "tritan",
                        other => other,
                    };
                    Some((short.to_owned(), roles(map)?))
                })
                .collect::<Option<_>>()?,
        },
        comparisons: raw["comparisons"]
            .as_array()?
            .iter()
            .map(|row| {
                Some(Comparison {
                    personality: text(&row["personality"])?,
                    colours: roles(&row["colours"])?,
                    minimum_contrast: row["minimumContrast"].as_f64().unwrap_or(0.0),
                })
            })
            .collect::<Option<_>>()?,
    })
}
/// The helper's catalog without the per-wallpaper palettes; `thumbnail` maps a JPEG to a PNG.
pub fn catalog_reply(
    raw: &Value,
    thumbnails: &mut dyn FnMut(&[&str]) -> Vec<String>,
) -> Option<ColorsCatalog> {
    let rows: Vec<&Value> = raw["entries"]
        .as_array()?
        .iter()
        .take(MAX_ENTRIES)
        .collect();
    let sources: Vec<&str> = rows
        .iter()
        .map(|row| row["thumbnail"].as_str().unwrap_or(""))
        .collect();
    let made = thumbnails(&sources);
    let mut entries = Vec::new();
    for (row, thumbnail) in rows.into_iter().zip(made) {
        entries.push(CatalogEntry {
            path: text(&row["path"])?,
            name: text(&row["name"]).unwrap_or_default(),
            dynamic: row["dynamic"].as_bool().unwrap_or(false),
            thumbnail,
        });
    }
    let list = |value: &Value, limit: usize| -> Vec<Value> {
        value
            .as_array()
            .map(|rows| rows.iter().take(limit).cloned().collect())
            .unwrap_or_default()
    };
    let choices: Map<String, Value> = raw["choices"]
        .as_object()
        .map(|all| {
            all.iter()
                .filter(|(path, _)| entries.iter().any(|entry| &entry.path == *path))
                .map(|(path, value)| (path.clone(), value.clone()))
                .collect()
        })
        .unwrap_or_default();
    let mut catalog = ColorsCatalog {
        current: text(&raw["current"]).unwrap_or_default(),
        entries,
        favorites: list(&raw["favorites"], 64),
        history: list(&raw["history"], 20),
        choices,
    };
    // Stay far below the frame limit whatever the stored values hold.
    while serde_json::to_vec(&catalog).map_or(usize::MAX, |bytes| bytes.len()) > REPLY_LIMIT {
        if let Some(key) = catalog.choices.keys().next().cloned() {
            catalog.choices.remove(&key);
        } else if catalog.entries.pop().is_none() {
            return None;
        }
    }
    Some(catalog)
}
/// Targets of `theme-targets.json`: the published file (with status) or the installed manifest.
pub fn targets_reply(paths: &Paths) -> Option<ColorsTargets> {
    let read = |path: PathBuf| -> Option<Value> {
        let meta = std::fs::metadata(&path).ok()?;
        (meta.is_file() && meta.len() <= TARGET_FILE_LIMIT)
            .then(|| serde_json::from_slice(&std::fs::read(path).ok()?).ok())
            .flatten()
    };
    let document = read(paths.state.join("theme-targets.json"))
        .or_else(|| read(paths.assets().join("theme-targets.json")))?;
    let targets = document["targets"]
        .as_array()?
        .iter()
        .take(MAX_TARGETS)
        .filter_map(|row| {
            let id = text(&row["id"])?;
            let status =
                serde_json::from_value(row["status"].clone()).unwrap_or(TargetStatus::NotPublished);
            let label = row["labelId"]
                .as_str()
                .and_then(crate::i18n::catalog_text)
                .unwrap_or_else(|| id.clone());
            Some(ColorsTarget { id, label, status })
        })
        .collect();
    Some(ColorsTargets { targets })
}

// ------------------------------------------------------------- dispatch

pub async fn dispatch(
    hub: Shared,
    method: &str,
    params: Value,
) -> std::result::Result<Value, Failure> {
    if hub.borrow().safe {
        return Err(failure(
            ErrorCode::PermissionDenied,
            "Colors is disabled in safe mode",
            false,
        )
        .dispatch_state(false));
    }
    if hub.borrow().settings.is_none() {
        return Err(unavailable("Colors is unavailable"));
    }
    let env = env()?;
    let reply = match method {
        "colors.catalog" => {
            serde_json::from_value::<nacre_protocol::pill_controls::PillEmpty>(params)
                .map_err(|_| invalid())?;
            serde_json::to_value(catalog(&env).await?)
        }
        "colors.preview" => {
            let request: ColorsPreview = serde_json::from_value(params).map_err(|_| invalid())?;
            request.validate()?;
            serde_json::to_value(preview(env, request).await?)
        }
        "colors.apply" | "colors.export" => {
            let request: ColorsId = serde_json::from_value(params).map_err(|_| invalid())?;
            request.validate()?;
            let action = method.trim_start_matches("colors.").to_owned();
            let input = json!({"id": request.id}).to_string() + "\n";
            let value = action_run(
                env,
                vec!["colors".into(), action.clone()],
                Some(input),
                true,
            )
            .await?;
            let unexpected = || io("Colors engine returned an unexpected result");
            if action == "apply" {
                serde_json::to_value(
                    serde_json::from_value::<ColorsApplied>(value).map_err(|_| unexpected())?,
                )
            } else {
                serde_json::to_value(
                    serde_json::from_value::<ColorsExported>(value).map_err(|_| unexpected())?,
                )
            }
        }
        "colors.targets" => {
            serde_json::from_value::<nacre_protocol::pill_controls::PillEmpty>(params)
                .map_err(|_| invalid())?;
            let paths = env.paths.clone();
            let targets = tokio::task::spawn_blocking(move || targets_reply(&paths))
                .await
                .ok()
                .flatten()
                .ok_or_else(|| unavailable("Application colour targets are unavailable"))?;
            serde_json::to_value(targets)
        }
        "colors.set_target" => {
            let request: ColorsSetTarget = serde_json::from_value(params).map_err(|_| invalid())?;
            request.validate()?;
            let known = targets_reply(&env.paths)
                .is_some_and(|all| all.targets.iter().any(|t| t.id == request.id));
            if !known {
                return Err(invalid());
            }
            let args = vec![
                "app-theme".into(),
                request.id,
                if request.enabled { "true" } else { "false" }.into(),
            ];
            action_run(env, args, None, false).await?;
            serde_json::to_value(ColorsChanged { changed: true })
        }
        _ => return Err(failure(ErrorCode::UnknownMethod, "Unknown method", false)),
    };
    reply.map_err(|_| io("Colors reply could not be encoded"))
}

async fn catalog(env: &Env) -> std::result::Result<ColorsCatalog, Failure> {
    let output = engine(env, &["colors", "catalog"], None, CATALOG_LIMIT).await?;
    let raw = engine_value(&output)?;
    let (thumbs, allowed) = (env.thumbs(), env.paths.cache.clone());
    tokio::task::spawn_blocking(move || {
        catalog_reply(&raw, &mut |sources| {
            pngs(
                thumbs.clone(),
                &allowed,
                sources,
                &[THUMB_SIDE],
                THUMB_BYTES,
            )
        })
    })
    .await
    .ok()
    .flatten()
    .ok_or_else(|| io("Colors engine returned an unexpected catalog"))
}

/// One preview at a time; a newer call cancels this one (waiting or running).
async fn preview(
    env: Env,
    request: ColorsPreview,
) -> std::result::Result<ColorsPreviewResult, Failure> {
    let mut newer = GENERATION.with(|generation| {
        generation.send_modify(|value| *value += 1);
        generation.subscribe()
    });
    let mine = *newer.borrow();
    let lane = PREVIEW_LANE.with(Rc::clone);
    let work = async {
        let _lane = lane.lock().await;
        run_preview(&env, &request).await
    };
    tokio::select! {
        result = work => result,
        _ = newer.wait_for(|value| *value != mine) => Err(
            failure(ErrorCode::Busy, "A newer preview replaced this one", true).dispatch_state(false)
        ),
    }
}
async fn run_preview(
    env: &Env,
    request: &ColorsPreview,
) -> std::result::Result<ColorsPreviewResult, Failure> {
    let input = Value::Object(request.request.clone()).to_string() + "\n";
    let output = engine(env, &["colors", "preview"], Some(input), WORK_LIMIT).await?;
    let raw = engine_value(&output)?;
    let source = raw["thumbnail"].as_str().unwrap_or("").to_owned();
    let (posters, allowed) = (env.posters(), env.paths.cache.clone());
    let poster = tokio::task::spawn_blocking(move || {
        pngs(
            posters,
            &allowed,
            &[source.as_str()],
            &POSTER_SIDES,
            POSTER_BYTES,
        )
        .remove(0)
    })
    .await
    .map_err(|_| io("Colors preview image failed"))?;
    preview_reply(&raw, poster).ok_or_else(|| io("Colors engine returned an unexpected preview"))
}

/// An action child that a deadline does not cancel; one at a time.
async fn action_run(
    env: Env,
    args: Vec<String>,
    input: Option<String>,
    reply: bool,
) -> std::result::Result<Value, Failure> {
    let busy = ACTION_BUSY.with(Rc::clone);
    if busy.replace(true) {
        return Err(
            failure(ErrorCode::Busy, "Another Colors action is running", true)
                .dispatch_state(false),
        );
    }
    let task = tokio::task::spawn_local(async move {
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let result = engine(&env, &args, input, WORK_LIMIT).await;
        busy.set(false);
        result
    });
    let output = task.await.map_err(|_| io("Colors action failed"))??;
    if output.code != 0 {
        return Err(if reply {
            refusal(&output)
        } else {
            io("Application colors could not be updated")
        });
    }
    if reply {
        last_json(&output).ok_or_else(|| io("Colors engine returned no result"))
    } else {
        Ok(Value::Null)
    }
}

#[cfg(test)]
mod tests;
