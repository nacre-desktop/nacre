fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let check = std::env::args().any(|arg| arg == "--check");
    for (path, content) in nacre_protocol::artifacts::generate() {
        let destination = root.join(&path);
        if check {
            if std::fs::read_to_string(&destination)? != content {
                return Err(format!("Stale generated protocol artifact: {path}").into());
            }
        } else {
            std::fs::create_dir_all(destination.parent().ok_or("Invalid export path")?)?;
            std::fs::write(destination, content)?;
        }
    }
    Ok(())
}
