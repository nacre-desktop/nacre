//! Private developer benchmark bus service, never installed.
use nacre_helper::Result;
use std::io::{self, BufRead, Write};
struct Wallet;
#[zbus::interface(name = "org.freedesktop.Secret.Collection")]
impl Wallet {
    #[zbus(property)]
    fn locked(&self) -> bool {
        false
    }
}
fn run() -> Result<()> {
    let connection = zbus::blocking::connection::Builder::address(
        std::env::args()
            .nth(1)
            .ok_or("Missing private bus address")?
            .as_str(),
    )?
    .name("org.freedesktop.secrets")?
    .name("org.freedesktop.login1")?
    .serve_at("/org/freedesktop/secrets/aliases/default", Wallet)?
    .build()?;
    println!("ready");
    io::stdout().flush()?;
    for line in io::stdin().lock().lines() {
        match line?.as_str() {
            "sleep" => connection.emit_signal(
                None::<&str>,
                "/org/freedesktop/login1",
                "org.freedesktop.login1.Manager",
                "PrepareForSleep",
                &(true,),
            )?,
            "resume" => connection.emit_signal(
                None::<&str>,
                "/org/freedesktop/login1",
                "org.freedesktop.login1.Manager",
                "PrepareForSleep",
                &(false,),
            )?,
            "quit" => break,
            _ => return Err("Unknown private fixture event".into()),
        }
    }
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
