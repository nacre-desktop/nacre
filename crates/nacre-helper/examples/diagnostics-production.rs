//! Developer-only real-router check inside the private parity namespace.
use nacre_helper::{diagnostics_native, Result};
fn main() -> Result<()> {
    if std::env::var("NACRE_PARITY_ROOT").as_deref() != Ok("/case")
        || std::env::var("HOME").as_deref() != Ok("/case/home")
    {
        return Err("Production report check requires the parity sandbox".into());
    }
    print!("{}", diagnostics_native::report(true)?);
    Ok(())
}
