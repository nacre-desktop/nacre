//! Pure palette/template computations use the pinned Orient libraries in-process.
use crate::Result;
use serde_json::{Map, Value};
pub fn render_values(template: &str, values: &Map<String, Value>) -> Result<String> {
    Ok(orient_core::template_engine::render_values(
        template, values, None, None, None, None,
    )?)
}
