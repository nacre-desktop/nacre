//! The desktop launcher model, independent of any UI or transport.
#![deny(unsafe_code)]
pub mod browser;
pub mod calculator;
pub mod config;
mod content;
pub mod context;
pub mod desktop;
pub mod emoji;
mod index;
pub mod model;
mod providers;
pub mod search;
mod software;
pub mod sources;
pub mod text;

pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Result<T> = std::result::Result<T, Error>;
