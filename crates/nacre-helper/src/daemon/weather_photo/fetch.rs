// Daemon-owned, bounded Wikimedia city-photo cache.
use crate::{paths::Paths, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs,
    io::{Read, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::PathBuf,
    sync::{Arc, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::process::Command;

const JSON_LIMIT: u64 = 256 * 1024;
const IMAGE_LIMIT: u64 = 2 * 1024 * 1024;
const MAX_AGE: f64 = 604800.0;
const CITY_API: &str = "https://en.wikipedia.org/w/api.php?action=query&format=json&formatversion=2&prop=pageimages%7Ccoordinates&coprimary=primary&colimit=1&redirects=1&pilicense=free&piprop=name&titles=";
const INFO_API: &str = "https://commons.wikimedia.org/w/api.php?action=query&format=json&formatversion=2&prop=imageinfo&iiprop=url%7Csize%7Cmime%7Cextmetadata&iiurlwidth=960&iiextmetadatafilter=Artist%7CAttribution%7CLicenseShortName&titles=";

fn test_endpoint_url(url: &str, origin: Option<&str>) -> bool {
    #[cfg(test)]
    {
        origin.is_some_and(|origin| {
            origin
                .strip_prefix("http://")
                .and_then(|s| s.parse::<std::net::SocketAddr>().ok())
                .is_some_and(|a| a.ip().is_loopback())
                && url.starts_with(&format!("{origin}/"))
        })
    }
    #[cfg(not(test))]
    {
        let _ = (url, origin);
        false
    }
}
pub use nacre_protocol::weather_photo::Location;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Photo {
    pub location: Location,
    pub checked: f64,
    pub author: String,
    pub license: String,
    pub source: String,
    pub path: String,
}

fn cache_dir(paths: &Paths) -> PathBuf {
    paths.cache.join("daemon-weather-photo")
}
fn safe_location(v: &Location) -> bool {
    !v.city.is_empty()
        && v.city.chars().count() <= 100
        && !v.city.chars().any(char::is_control)
        && v.latitude.is_finite()
        && (-90.0..=90.0).contains(&v.latitude)
        && v.longitude.is_finite()
        && (-180.0..=180.0).contains(&v.longitude)
}
fn plain(s: &str) -> String {
    let mut out = String::new();
    let mut tag = false;
    for c in s.chars() {
        match c {
            '<' => tag = true,
            '>' => tag = false,
            _ if !tag => out.push(c),
            _ => {}
        }
    }
    let decoded = html_escape::decode_html_entities(&out);
    decoded
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
fn image_url(s: &str) -> bool {
    const PREFIX: &str = "https://upload.wikimedia.org/wikipedia/commons/";
    if s.len() > 2048
        || !s.starts_with(PREFIX)
        || !s[PREFIX.len()..]
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_%()/!~*'.,+:-".contains(&b))
    {
        return false;
    }
    let b = s.as_bytes();
    let mut decoded = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            if i + 2 >= b.len() {
                return false;
            }
            let (Some(a), Some(c)) = (hex(b[i + 1]), hex(b[i + 2])) else {
                return false;
            };
            decoded.push(a * 16 + c);
            i += 3;
        } else {
            decoded.push(b[i]);
            i += 1;
        }
    }
    let Ok(decoded) = std::str::from_utf8(&decoded) else {
        return false;
    };
    !decoded.contains("..")
        && !decoded.contains(['\\', '\0'])
        && !decoded.chars().any(char::is_control)
}
fn hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}
fn source_url(s: &str) -> bool {
    if s.len() >= 2048 || !s.starts_with("https://commons.wikimedia.org/wiki/File") {
        return false;
    }
    let tail = &s["https://commons.wikimedia.org/wiki/File".len()..];
    let tail = tail
        .strip_prefix(':')
        .or_else(|| tail.strip_prefix("%3A"))
        .or_else(|| tail.strip_prefix("%3a"));
    tail.is_some_and(|t| {
        !t.is_empty()
            && !t
                .chars()
                .any(|c| c.is_whitespace() || matches!(c, '?' | '#' | '\\') || c.is_control())
    })
}
fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}
fn city_title(v: &Value, l: &Location) -> Option<String> {
    let p = &v["query"]["pages"][0];
    let point = &p["coordinates"][0];
    let lat = point["lat"].as_f64()?;
    let lon = point["lon"].as_f64()?;
    if p["ns"].as_i64()? != 0
        || point["globe"].as_str()? != "earth"
        || !lat.is_finite()
        || !lon.is_finite()
        || lat.abs() > 90.
        || lon.abs() > 180.
    {
        return None;
    }
    let dx =
        ((lon - l.longitude + 540.) % 360. - 180.) * ((lat + l.latitude).to_radians() / 2.).cos();
    let dy = lat - l.latitude;
    if (dx * dx + dy * dy).sqrt() * 111.2 > 50. {
        return None;
    }
    let title = p["pageimage"].as_str()?;
    (title.chars().count() < 240
        && [".jpg", ".jpeg", ".png"]
            .iter()
            .any(|x| title.to_ascii_lowercase().ends_with(x)))
    .then(|| title.to_owned())
}
struct Meta {
    url: String,
    source: String,
    author: String,
    license: String,
    mime: String,
}
fn metadata(v: &Value, test_origin: Option<&str>) -> Option<Meta> {
    let i = &v["query"]["pages"][0]["imageinfo"][0];
    let mime = i["mime"].as_str()?;
    if mime != "image/jpeg" && mime != "image/png" {
        return None;
    }
    let url = i["thumburl"]
        .as_str()
        .filter(|s| !s.is_empty())
        .or_else(|| i["url"].as_str())?
        .split('?')
        .next()?
        .to_owned();
    let source = i["descriptionurl"].as_str()?.to_owned();
    let w = i["thumbwidth"]
        .as_u64()
        .filter(|n| *n != 0)
        .or_else(|| i["width"].as_u64())?;
    let h = i["thumbheight"]
        .as_u64()
        .filter(|n| *n != 0)
        .or_else(|| i["height"].as_u64())?;
    if !(image_url(&url) || test_endpoint_url(&url, test_origin))
        || !source_url(&source)
        || w == 0
        || h == 0
        || w > 1400
        || h > 1400
        || w * h > 1_960_000
    {
        return None;
    }
    let x = &i["extmetadata"];
    let author = plain(
        x["Attribution"]["value"]
            .as_str()
            .filter(|s| !s.is_empty())
            .or_else(|| x["Artist"]["value"].as_str())
            .unwrap_or(""),
    );
    let license = plain(x["LicenseShortName"]["value"].as_str().unwrap_or(""));
    let l = license.to_ascii_lowercase();
    let ok = l == "cc0"
        || l == "cc0 1.0"
        || l == "public domain"
        || ["1.0", "2.0", "2.5", "3.0", "4.0"]
            .iter()
            .any(|n| l == format!("cc by {n}") || l == format!("cc by-sa {n}"));
    if author.is_empty()
        || author.chars().count() > 160
        || license.is_empty()
        || license.chars().count() > 40
        || !ok
    {
        return None;
    }
    Some(Meta {
        url,
        source,
        author,
        license,
        mime: mime.to_owned(),
    })
}
fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}
fn decode(bytes: &[u8]) -> Result<()> {
    let result = validate(bytes);
    // The validator and its decoded pixels have both gone out of scope.
    crate::process::reclaim_unused_memory();
    result
}
fn validate(bytes: &[u8]) -> Result<()> {
    let fmt = image::guess_format(bytes)?;
    if fmt != image::ImageFormat::Jpeg && fmt != image::ImageFormat::Png {
        return Err("unsupported photo format".into());
    }
    let mut r = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(1400);
    limits.max_image_height = Some(1400);
    limits.max_alloc = Some(32 * 1024 * 1024);
    r.limits(limits);
    let i = r.decode()?;
    if i.width() > 1400 || i.height() > 1400 {
        return Err("photo dimensions exceed limit".into());
    }
    Ok(())
}
fn cached(paths: &Paths, l: &Location) -> Option<Photo> {
    let d = cache_dir(paths);
    if fs::symlink_metadata(&d).ok()?.file_type().is_symlink() {
        return None;
    }
    let m = d.join("photo.json");
    let p = d.join("city.image");
    if fs::symlink_metadata(&m).ok()?.file_type().is_symlink() {
        return None;
    }
    let f = super::super::active_wallpaper::private_file(&m, 12000).ok()?;
    let mut b = Vec::new();
    f.take(12001).read_to_end(&mut b).ok()?;
    if b.len() > 12000 {
        return None;
    }
    let mut x: Photo = serde_json::from_slice(&b).ok()?;
    let n = now();
    if x.location != *l
        || !x.checked.is_finite()
        || x.checked > n + 60.
        || n - x.checked >= MAX_AGE
        || !source_url(&x.source)
        || x.author.chars().count() > 160
        || x.license.chars().count() > 40
    {
        return None;
    }
    if fs::symlink_metadata(&p).ok()?.file_type().is_symlink() {
        return None;
    }
    let f = super::super::active_wallpaper::private_file(&p, IMAGE_LIMIT).ok()?;
    let mut b = Vec::new();
    f.take(IMAGE_LIMIT + 1).read_to_end(&mut b).ok()?;
    if b.len() as u64 > IMAGE_LIMIT || decode(&b).is_err() {
        return None;
    }
    x.path = p.to_string_lossy().into_owned();
    Some(x)
}
async fn request(
    url: &str,
    limit: u64,
    kind: &str,
    d: &std::path::Path,
    test_origin: Option<&str>,
) -> Result<Vec<u8>> {
    if !url.starts_with("https://") {
        #[cfg(test)]
        if !test_endpoint_url(url, test_origin) {
            return Err("photo URL is not HTTPS or approved test loopback".into());
        }
        #[cfg(not(test))]
        return Err("photo URL is not HTTPS".into());
    }
    let directory = d.to_owned();
    let tmp = crate::process::offload(move || {
        // A fixed temporary name also bounds disk use across abrupt daemon exits.
        match fs::remove_file(directory.join("download.part")) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        let tmp = tempfile::Builder::new()
            .prefix("download")
            .suffix(".part")
            .rand_bytes(0)
            .tempfile_in(directory)?;
        super::super::active_wallpaper::private_file(tmp.path(), 0)?;
        Ok::<_, crate::Error>(tmp)
    })
    .await??;
    let path = tmp.path().to_owned();
    let size = limit.to_string();
    #[cfg(test)]
    let proto = if test_origin.is_some() {
        "=https,http"
    } else {
        "=https"
    };
    #[cfg(not(test))]
    let proto = {
        let _ = test_origin;
        "=https"
    };
    let child = Command::new("nice")
        .args([
            "-n",
            "10",
            "curl",
            "-q",
            "--proto",
            proto,
            "--max-redirs",
            "0",
            "--fail",
            "--silent",
            "--show-error",
            "--connect-timeout",
            "3",
            "--max-time",
            "10",
            "--max-filesize",
            &size,
            "--user-agent",
            "Nacre/1.0 (weather photos)",
            "--write-out",
            "%{content_type}",
            "--output",
        ])
        .arg(&path)
        .arg("--header")
        .arg(format!("Accept: {kind}"))
        .arg("--url")
        .arg(url)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .output();
    let o = tokio::time::timeout(std::time::Duration::from_secs(11), child).await??;
    if !o.status.success()
        || String::from_utf8_lossy(&o.stdout)
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            != kind
    {
        return Err("photo request failed validation".into());
    }
    crate::process::offload(move || -> Result<Vec<u8>> {
        let f = tmp.reopen()?;
        let mut bytes = Vec::new();
        f.take(limit + 1).read_to_end(&mut bytes)?;
        if bytes.len() as u64 > limit {
            return Err("photo response too large".into());
        }
        Ok(bytes)
    })
    .await?
}
async fn load_at(
    paths: Paths,
    l: Location,
    city_api: &str,
    info_api: &str,
    test_origin: Option<&str>,
) -> Result<Photo> {
    static LANE: OnceLock<Arc<tokio::sync::Mutex<()>>> = OnceLock::new();
    let lane = LANE
        .get_or_init(|| Arc::new(tokio::sync::Mutex::new(())))
        .clone();
    let guard = lane.lock_owned().await;
    if !safe_location(&l) {
        return Err("invalid weather location".into());
    }
    if let Some(x) = crate::process::offload({
        let paths = paths.clone();
        let l = l.clone();
        move || cached(&paths, &l)
    })
    .await?
    {
        return Ok(x);
    }
    let d = cache_dir(&paths);
    let prep = d.clone();
    crate::process::offload(move || -> Result<()> {
        fs::create_dir_all(&prep)?;
        if fs::symlink_metadata(&prep)?.file_type().is_symlink() {
            return Err("refusing linked photo cache".into());
        }
        fs::set_permissions(&prep, fs::Permissions::from_mode(0o700))?;
        Ok(())
    })
    .await??;
    let b = request(
        &format!("{city_api}{}", encode(&l.city)),
        JSON_LIMIT,
        "application/json",
        &d,
        test_origin,
    )
    .await?;
    let l2 = l.clone();
    let title = crate::process::offload(move || -> Result<String> {
        city_title(&serde_json::from_slice(&b)?, &l2).ok_or_else(|| "no nearby city image".into())
    })
    .await??;
    let b = request(
        &format!("{info_api}{}", encode(&format!("File:{title}"))),
        JSON_LIMIT,
        "application/json",
        &d,
        test_origin,
    )
    .await?;
    let origin_owned = test_origin.map(str::to_owned);
    let m = crate::process::offload(move || -> Result<Meta> {
        metadata(&serde_json::from_slice(&b)?, origin_owned.as_deref())
            .ok_or_else(|| "invalid image metadata".into())
    })
    .await??;
    let kind = m.mime.as_str();
    let b = request(&m.url, IMAGE_LIMIT, kind, &d, test_origin).await?;
    let b = crate::process::offload(move || -> Result<Vec<u8>> {
        decode(&b)?;
        Ok(b)
    })
    .await??;
    let path = d.join("city.image");
    let mp = d.join("photo.json");
    crate::process::offload(move || -> Result<Photo> {
        let _guard = guard;
        let t = d.join("city.image.part");
        if fs::symlink_metadata(&t).is_ok() {
            fs::remove_file(&t)?;
        }
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&t)?;
        f.write_all(&b)?;
        f.sync_all()?;
        // The record must never describe a previous city after an interrupted install.
        match fs::remove_file(&mp) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        fs::rename(t, &path)?;
        let x = Photo {
            location: l,
            checked: now(),
            author: m.author,
            license: m.license,
            source: m.source,
            path: path.to_string_lossy().into_owned(),
        };
        let b = serde_json::to_vec(&x)?;
        if b.len() > 12000 {
            return Err("photo metadata too large".into());
        }
        let t = d.join("photo.json.part");
        if fs::symlink_metadata(&t).is_ok() {
            fs::remove_file(&t)?;
        }
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&t)?;
        f.write_all(&b)?;
        f.sync_all()?;
        fs::rename(t, mp)?;
        Ok(x)
    })
    .await?
}
pub async fn load(paths: Paths, location: Location) -> Result<Photo> {
    load_at(paths, location, CITY_API, INFO_API, None).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::{
        io::Cursor,
        sync::atomic::{AtomicUsize, Ordering},
    };
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    struct Reply {
        status: u16,
        content_type: &'static str,
        body: Vec<u8>,
    }
    async fn fake_server(
        replies: Vec<Reply>,
        pause: bool,
    ) -> (String, Arc<AtomicUsize>, tokio::task::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let origin = format!("http://{}", listener.local_addr().unwrap());
        let count = Arc::new(AtomicUsize::new(0));
        let seen = count.clone();
        let task_origin = origin.clone();
        let task = tokio::spawn(async move {
            for reply in replies {
                let (mut stream, _) = listener.accept().await.unwrap();
                seen.fetch_add(1, Ordering::SeqCst);
                let mut request = Vec::new();
                let mut buf = [0; 1024];
                loop {
                    let n = stream.read(&mut buf).await.unwrap();
                    if n == 0 {
                        break;
                    }
                    request.extend_from_slice(&buf[..n]);
                    if request.windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                }
                if pause {
                    std::future::pending::<()>().await;
                }
                let reason = if reply.status == 200 {
                    "OK"
                } else {
                    "Not Found"
                };
                let body = String::from_utf8(reply.body.clone())
                    .ok()
                    .map(|b| b.replace("__ORIGIN__", &task_origin).into_bytes())
                    .unwrap_or(reply.body);
                let head=format!("HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",reply.status,reason,reply.content_type,body.len());
                if stream.write_all(head.as_bytes()).await.is_ok() {
                    let _ = stream.write_all(&body).await;
                }
            }
        });
        (origin, count, task)
    }
    fn png() -> Vec<u8> {
        let image = image::DynamicImage::ImageRgb8(image::RgbImage::new(1, 1));
        let mut c = Cursor::new(Vec::new());
        image.write_to(&mut c, image::ImageFormat::Png).unwrap();
        c.into_inner()
    }
    fn loc() -> Location {
        Location {
            city: "Oslo".into(),
            latitude: 59.91,
            longitude: 10.75,
        }
    }
    fn city_json() -> Vec<u8> {
        serde_json::to_vec(&json!({"query":{"pages":[{"ns":0,"pageimage":"Oslo.png","coordinates":[{"lat":59.91,"lon":10.75,"globe":"earth"}]}]}})).unwrap()
    }
    fn info_json() -> Vec<u8> {
        serde_json::to_vec(&json!({"query":{"pages":[{"imageinfo":[{"mime":"image/png","thumburl":"__ORIGIN__/image.png","descriptionurl":"https://commons.wikimedia.org/wiki/File:Oslo.png","thumbwidth":1,"thumbheight":1,"extmetadata":{"Artist":{"value":"Artist"},"LicenseShortName":{"value":"CC BY 4.0"}}}]}]}})).unwrap()
    }
    async fn run(paths: Paths, origin: &str) -> Result<Photo> {
        load_at(
            paths,
            loc(),
            &format!("{origin}/city?"),
            &format!("{origin}/info?"),
            Some(origin),
        )
        .await
    }
    fn reply(body: Vec<u8>, content_type: &'static str) -> Reply {
        Reply {
            status: 200,
            content_type,
            body,
        }
    }
    fn temp_paths() -> (tempfile::TempDir, Paths) {
        let t = tempfile::tempdir().unwrap();
        let p = Paths::new(t.path());
        (t, p)
    }

    #[test]
    fn selection_preserves_distance_licence_dimensions_and_url_guards() {
        let mut city: Value = serde_json::from_slice(&city_json()).unwrap();
        assert_eq!(city_title(&city, &loc()).as_deref(), Some("Oslo.png"));
        city["query"]["pages"][0]["coordinates"][0]["lat"] = json!(0);
        assert!(city_title(&city, &loc()).is_none());
        let mut info: Value = serde_json::from_slice(&info_json()).unwrap();
        info["query"]["pages"][0]["imageinfo"][0]["thumburl"] =
            json!("https://upload.wikimedia.org/wikipedia/commons/a/ab/City.png");
        assert!(metadata(&info, None).is_some());
        for url in [
            "https://example.com/City.png",
            "https://upload.wikimedia.org/wikipedia/commons/%2e%2e/City.png",
            "https://upload.wikimedia.org/wikipedia/commons/a%5cb.png",
        ] {
            assert!(!image_url(url));
        }
        info["query"]["pages"][0]["imageinfo"][0]["thumbwidth"] = json!(1401);
        assert!(metadata(&info, None).is_none());
        info["query"]["pages"][0]["imageinfo"][0]["thumbwidth"] = json!(1);
        info["query"]["pages"][0]["imageinfo"][0]["extmetadata"]["LicenseShortName"]["value"] =
            json!("All rights reserved");
        assert!(metadata(&info, None).is_none());
    }

    #[tokio::test]
    async fn fake_photo_is_validated_and_cached() {
        let (_t, paths) = temp_paths();
        let (origin, count, server) = fake_server(
            vec![
                reply(city_json(), "application/json"),
                reply(info_json(), "application/json"),
                reply(png(), "image/png"),
            ],
            false,
        )
        .await;
        let photo = run(paths.clone(), &origin).await.unwrap();
        assert_eq!(photo.location, loc());
        assert_eq!(photo.author, "Artist");
        assert_eq!(count.load(Ordering::SeqCst), 3);
        assert!(std::path::Path::new(&photo.path).is_file());
        assert_eq!(run(paths, &origin).await.unwrap().path, photo.path);
        assert_eq!(count.load(Ordering::SeqCst), 3);
        server.abort();
    }

    #[tokio::test]
    async fn fake_404_is_rejected() {
        let (_t, p) = temp_paths();
        let (o, n, s) = fake_server(
            vec![Reply {
                status: 404,
                content_type: "text/plain",
                body: b"missing".to_vec(),
            }],
            false,
        )
        .await;
        assert!(run(p, &o).await.is_err());
        assert_eq!(n.load(Ordering::SeqCst), 1);
        s.abort();
    }
    #[tokio::test]
    async fn fake_oversize_json_is_rejected() {
        let (_t, p) = temp_paths();
        let (o, n, s) = fake_server(
            vec![reply(
                vec![b'x'; JSON_LIMIT as usize + 1],
                "application/json",
            )],
            false,
        )
        .await;
        assert!(run(p, &o).await.is_err());
        assert_eq!(n.load(Ordering::SeqCst), 1);
        s.abort();
    }
    #[tokio::test]
    async fn fake_slow_request_is_cancellable() {
        let (_t, p) = temp_paths();
        let (o, n, s) = fake_server(vec![reply(Vec::new(), "application/json")], true).await;
        let mut job = Box::pin(run(p.clone(), &o));
        tokio::select! {
            result = &mut job => panic!("slow fetch unexpectedly completed: {result:?}"),
            _ = tokio::time::timeout(std::time::Duration::from_secs(3), async {
                while n.load(Ordering::SeqCst) == 0 {tokio::time::sleep(std::time::Duration::from_millis(5)).await;}
            }) => {},
        }
        assert_eq!(n.load(Ordering::SeqCst), 1);
        let result = tokio::time::timeout(std::time::Duration::from_millis(150), &mut job).await;
        assert!(result.is_err());
        drop(job);
        assert_eq!(n.load(Ordering::SeqCst), 1);
        assert_eq!(
            fs::read_dir(cache_dir(&p)).unwrap().count(),
            0,
            "cancelled downloads leave no temporary cache files"
        );
        s.abort();
    }
    #[tokio::test]
    async fn fake_bad_content_type_and_oversize_image_are_rejected() {
        for (body, kind) in [
            (png(), "text/html"),
            (vec![0; IMAGE_LIMIT as usize + 1], "image/png"),
        ] {
            let (_t, p) = temp_paths();
            let (o, n, s) = fake_server(
                vec![
                    reply(city_json(), "application/json"),
                    reply(info_json(), "application/json"),
                    reply(body, kind),
                ],
                false,
            )
            .await;
            assert!(run(p, &o).await.is_err());
            assert_eq!(n.load(Ordering::SeqCst), 3);
            s.abort();
        }
    }
    #[tokio::test]
    async fn fake_non_image_bytes_are_rejected() {
        let (_t, p) = temp_paths();
        let (o, n, s) = fake_server(
            vec![
                reply(city_json(), "application/json"),
                reply(info_json(), "application/json"),
                reply(b"not an image".to_vec(), "image/png"),
            ],
            false,
        )
        .await;
        assert!(run(p, &o).await.is_err());
        assert_eq!(n.load(Ordering::SeqCst), 3);
        s.abort();
    }
}
