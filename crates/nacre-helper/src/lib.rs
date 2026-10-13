//! Native desktop helper boundaries. Orient remains an independent MIT dependency.
#![forbid(unsafe_code)]
pub mod orient;
pub mod paths;
pub type Error = Box<dyn std::error::Error + Send + Sync>;
pub type Result<T> = std::result::Result<T, Error>;
