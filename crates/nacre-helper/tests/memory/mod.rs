//! Linux process-level guard: raster bursts must not become daemon caches.
use super::*;
type Memory = (u64, u64, u64); // RSS, anonymous, clean file PSS, in KiB.

struct LoginManager;
#[zbus::interface(name = "org.freedesktop.login1.Manager")]
impl LoginManager {
    fn get_user(&self, _uid: u32) -> zvariant::OwnedObjectPath {
        "/org/freedesktop/login1/user/fixture".try_into().unwrap()
    }
}
struct LoginUser;
#[zbus::interface(name = "org.freedesktop.login1.User")]
impl LoginUser {
    #[zbus(property)]
    fn display(&self) -> (String, zvariant::OwnedObjectPath) {
        (String::new(), "/".try_into().unwrap())
    }
}

fn memory(pid: u32) -> Result<Memory> {
    let text = fs::read_to_string(format!("/proc/{pid}/smaps_rollup"))?;
    let field = |name: &str| -> u64 {
        text.lines()
            .find_map(|line| line.strip_prefix(name))
            .and_then(|value| value.split_whitespace().next()?.parse().ok())
            .unwrap_or(0)
    };
    if field("SwapPss:") != 0 || field("Pss_Shmem:") != 0 {
        return Err("invalid memory sample: nonzero SwapPss or Pss_Shmem".into());
    }
    Ok((field("Rss:"), field("Anonymous:"), field("Pss_File:")))
}

struct Samples {
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<Result<Vec<Memory>>>>,
}
impl Samples {
    fn start(pid: u32) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let done = stop.clone();
        let worker = thread::spawn(move || {
            let mut values = Vec::new();
            while !done.load(Ordering::Relaxed) {
                values.push(memory(pid)?);
                thread::sleep(Duration::from_millis(50));
            }
            Ok(values)
        });
        Self {
            stop,
            worker: Some(worker),
        }
    }
    fn finish(mut self) -> Result<Vec<Memory>> {
        self.stop.store(true, Ordering::Relaxed);
        self.worker
            .take()
            .unwrap()
            .join()
            .map_err(|_| "memory sampler panicked")?
    }
}
impl Drop for Samples {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn call(reader: &mut BufReader<UnixStream>, id: u64, method: &str, params: Value) -> Result<Value> {
    let deadline = Instant::now() + Duration::from_secs(30);
    writeln!(
        reader.get_mut(),
        "{}",
        json!({"t":"call","id":id,"m":method,"p":params})
    )?;
    loop {
        if Instant::now() >= deadline {
            return Err(format!("{method} reply timed out").into());
        }
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return Err("daemon disconnected".into());
        }
        let message: Value = serde_json::from_str(&line)?;
        if message["t"] == "ret" && message["id"] == id {
            assert!(message.get("ok").is_some(), "{message}");
            return Ok(message["ok"].clone());
        }
    }
}

fn wait_lock(root: &Path, wallpaper: &Path) -> Result<()> {
    let ready = root.join("cache/nacre/lock-ready/dashboard.json");
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        if fs::read(&ready)
            .ok()
            .and_then(|raw| serde_json::from_slice::<Value>(&raw).ok())
            .is_some_and(|data| data["wallpaper"] == json!(wallpaper))
        {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "lock did not prepare: {}",
                fs::read_to_string(root.join("daemon.log"))?
            )
            .into());
        }
        thread::sleep(Duration::from_millis(100));
    }
}

#[test]
#[ignore = "run in isolation after builds: cargo test --test daemon memory:: -- --ignored --nocapture"]
fn catalogue_and_lock_raster_bursts_settle_below_sixty_mib() -> Result<()> {
    let home = tempfile::Builder::new().prefix("mem-").tempdir()?;
    let root = home.path();
    fs::set_permissions(root, fs::Permissions::from_mode(0o700))?;
    let library = root.join("Pictures/Wallpapers");
    fs::create_dir_all(&library)?;
    let image = library.join("00.jpg");
    image::RgbImage::from_fn(3840, 2160, |x, y| {
        image::Rgb([(x % 251) as u8, (y % 251) as u8, ((x + y) % 251) as u8])
    })
    .save(&image)?;
    for index in 1..20 {
        fs::copy(&image, library.join(format!("{index:02}.jpg")))?;
    }
    // Exercise a later full-resolution render after startup's unrelated trims
    // have finished; an initial render alone can hide arena retention.
    let late_image = root.join("large.png");
    image::RgbaImage::from_fn(6000, 4000, |x, y| {
        image::Rgba([(x % 251) as u8, (y % 251) as u8, ((x + y) % 251) as u8, 255])
    })
    .save(&late_image)?;
    // A real (tiny) video also traverses the poster path; no ambient media is read.
    let video = Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=c=blue:s=320x180:d=0.1",
            "-y",
        ])
        .arg(library.join("video.mp4"))
        .status()?;
    assert!(video.success());
    let config = root.join("config/nacre");
    let state = root.join("state/nacre");
    fs::create_dir_all(&config)?;
    fs::create_dir_all(&state)?;
    fs::write(
        config.join("desktop.json"),
        serde_json::to_vec(&json!({
            "version":3,"lockMedia":false
        }))?,
    )?;
    fs::write(
        config.join("cli.json"),
        serde_json::to_vec(&json!({"wallpaper":{"directory":library}}))?,
    )?;
    fs::write(
        config.join("dashboard-extras.json"),
        br#"{"version":1,"weatherPhotos":true}"#,
    )?;
    // A private logind permits the photo lease without any host-bus access.
    let mut bus = Process(
        Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .spawn()?,
    );
    let mut address = String::new();
    BufReader::new(bus.0.stdout.take().unwrap()).read_line(&mut address)?;
    let _login = zbus::blocking::connection::Builder::address(address.trim())?
        .name("org.freedesktop.login1")?
        .serve_at("/org/freedesktop/login1", LoginManager)?
        .serve_at("/org/freedesktop/login1/user/fixture", LoginUser)?
        .build()?;
    let cache = root.join("cache/nacre");
    let photo_cache = cache.join("daemon-weather-photo");
    fs::create_dir_all(&photo_cache)?;
    let checked = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs_f64();
    let mut weather: Value = serde_json::from_str(
        include_str!("../../../../protocol/fixtures/core-dashboard-weather.jsonl")
            .lines()
            .next()
            .unwrap(),
    )?;
    weather = weather["data"]["current"].clone();
    weather["checked"] = json!(checked);
    weather["localTime"] = weather["local_time"].clone();
    weather["feelsLike"] = weather["feels_like"].clone();
    weather["isDay"] = weather["is_day"].clone();
    weather["code"] = json!(0);
    weather["hours"] = json!([]);
    weather["days"] = json!([]);
    fs::write(cache.join("weather.json"), serde_json::to_vec(&weather)?)?;
    let photo = photo_cache.join("city.image");
    image::RgbImage::from_pixel(1400, 1400, image::Rgb([20, 80, 130]))
        .save_with_format(&photo, image::ImageFormat::Png)?;
    fs::write(
        photo_cache.join("photo.json"),
        serde_json::to_vec(&json!({
            "location":{"city":"Fixture location","latitude":60.0,"longitude":5.0},
            "checked":checked,"author":"Fixture artist","license":"CC BY 4.0",
            "source":"https://commons.wikimedia.org/wiki/File:Fixture.png","path":photo
        }))?,
    )?;
    let bin = root.join("bin");
    fs::create_dir(&bin)?;
    fs::write(bin.join("parec"), "#!/usr/bin/python\nimport sys,time\nwhile True:\n sys.stdout.buffer.write(b'\\x01\\x10'*800); sys.stdout.buffer.flush(); time.sleep(1/60)\n")?;
    fs::set_permissions(bin.join("parec"), fs::Permissions::from_mode(0o755))?;
    let mut record: Value = serde_json::from_str(include_str!(
        "../../../../nacre/shell-tools/reference-style.json"
    ))?;
    record["mode"] = json!("dark");
    record["poster"] = json!(image);
    fs::write(
        state.join("presentation.json"),
        serde_json::to_vec(&record)?,
    )?;
    let source_path = root.join("niri.sock");
    let _source = Source::new(source_path.clone())?;
    let folder = root.join("private");
    fs::create_dir(&folder)?;
    fs::set_permissions(&folder, fs::Permissions::from_mode(0o700))?;
    let endpoint = folder.join("daemon.sock");
    let notify = root.join("notify.sock");
    let notifications = UnixDatagram::bind(&notify)?;
    notifications.set_read_timeout(Some(Duration::from_secs(5)))?;
    let log = fs::File::create(root.join("daemon.log"))?;
    let mut daemon = command(root, &source_path, &endpoint);
    if let Some(binary) = std::env::var_os("NACRE_MEMORY_BINARY") {
        daemon = Command::new(binary);
        environment(&mut daemon, root, &source_path, &endpoint);
    }
    let mut child = Process(
        daemon
            .args(["daemon", "--notifications=daemon", "--behaviours=daemon"])
            .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
            .env("DBUS_SYSTEM_BUS_ADDRESS", address.trim())
            .env("DBUS_SESSION_BUS_ADDRESS", address.trim())
            .env(
                "NACRE_HELPER_TOOLS",
                Path::new(env!("CARGO_MANIFEST_DIR")).join("../../nacre/shell-tools"),
            )
            .env("NOTIFY_SOCKET", &notify)
            .stderr(log)
            .stdout(Stdio::null())
            .spawn()?,
    );
    let samples = Samples::start(child.0.id());
    notifications.recv(&mut [0; 256])?;
    let startup = memory(child.0.id())?;
    eprintln!("N0076 startup phase RSS/anon KiB: {startup:?}");
    wait_lock(root, &image)?;
    thread::sleep(Duration::from_secs(3));
    assert_eq!(
        status(root, &source_path, &endpoint)?["services"]["notifications"]["status"],
        "running"
    );
    let lock = memory(child.0.id())?;
    eprintln!("N0076 lock phase RSS/anon KiB: {lock:?}");
    let stream = UnixStream::connect(&endpoint)?;
    stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    let mut reader = BufReader::new(stream);
    writeln!(
        reader.get_mut(),
        "{}",
        json!({"t":"hello","proto":1,"client":"memory-fixture"})
    )?;
    reader.read_line(&mut String::new())?;
    writeln!(
        reader.get_mut(),
        "{}",
        json!({"t":"sub","topics":["wallpaper.active","audio.spectrum"]})
    )?;
    let spectrum_deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if Instant::now() >= spectrum_deadline {
            return Err("spectrum did not receive PCM".into());
        }
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return Err("spectrum lease disconnected".into());
        }
        let message: Value = serde_json::from_str(&line)?;
        if message["topic"] == "audio.spectrum" && message["data"]["health"]["available"] == true {
            break;
        }
    }
    eprintln!(
        "N0076 spectrum phase RSS/anon KiB: {:?}",
        memory(child.0.id())?
    );
    let catalog = call(&mut reader, 1, "wallpaper.catalogue", json!({"ids":[]}))?;
    eprintln!(
        "N0076 metadata phase RSS/anon KiB: {:?}",
        memory(child.0.id())?
    );
    let entries = catalog["entries"].as_array().ok_or("missing catalogue")?;
    assert_eq!(entries.len(), 21, "{catalog}");
    // Two visible image rows and the video exercise both raster paths. Enumerate
    // the full library, but keep this a retention guard rather than a throughput test.
    for (index, entry) in entries
        .iter()
        .enumerate()
        .filter(|(index, entry)| *index < 2 || entry["kind"] == "video")
    {
        let hydrated = call(
            &mut reader,
            index as u64 + 2,
            "wallpaper.catalogue",
            json!({"ids":[entry["id"]]}),
        )?;
        let row = hydrated["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["id"] == entry["id"])
            .unwrap();
        for field in ["thumbnail", "preview"] {
            assert!(
                row[field]
                    .as_str()
                    .is_some_and(|path| !path.is_empty() && Path::new(path).is_file()),
                "{row}"
            );
        }
    }
    eprintln!(
        "N0076 hydrated phase RSS/anon KiB: {:?}",
        memory(child.0.id())?
    );
    writeln!(
        reader.get_mut(),
        "{}",
        json!({"t":"sub","topics":["dashboard.weather_photo"]})
    )?;
    let photo_deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if Instant::now() >= photo_deadline {
            return Err(format!(
                "photo did not become ready: {}",
                fs::read_to_string(root.join("daemon.log"))?
            )
            .into());
        }
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return Err("photo lease disconnected".into());
        }
        let message: Value = serde_json::from_str(&line)?;
        if message["topic"] == "dashboard.weather_photo" && message["data"]["ready"] == true {
            break;
        }
    }
    eprintln!(
        "N0076 photo phase RSS/anon KiB: {:?}",
        memory(child.0.id())?
    );
    thread::sleep(Duration::from_secs(3));
    eprintln!("N0076 warm phase RSS/anon KiB: {:?}", memory(child.0.id())?);
    record["poster"] = json!(late_image);
    fs::write(
        state.join("presentation.json"),
        serde_json::to_vec(&record)?,
    )?;
    wait_lock(root, &late_image)?;
    thread::sleep(Duration::from_secs(3));
    let settled = memory(child.0.id())?;
    eprintln!("N0076 late-lock phase RSS/anon KiB: {settled:?}");
    let samples = samples.finish()?;
    assert!(!samples.is_empty());
    let peak = samples.iter().map(|value| value.1).max().unwrap();
    eprintln!(
        "N0076 CPU renderer, KiB: peak anon={peak}, settled RSS={}, anon={}, file PSS={}",
        settled.0, settled.1, settled.2
    );
    child.0.kill()?;
    child.0.wait()?;
    let log = fs::read_to_string(root.join("daemon.log"))?;
    assert!(
        !log.contains("Lock wallpaper texture:"),
        "wallpaper fell back: {log}"
    );
    assert!(peak < 512 * 1024, "unexpected raster peak: {peak} KiB");
    assert!(
        lock.1 <= 60 * 1024,
        "lock raster burst retained: {} KiB",
        lock.1
    );
    assert!(
        settled.1 <= 60 * 1024,
        "raster burst retained: {} KiB",
        settled.1
    );
    Ok(())
}
