//! The compatibility observer configures only its own compositor/X11 session.
use crate::{ErrorCode, Failure};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WidthsConfigure {
    #[schemars(length(min = 1, max = 256))]
    pub connection_id: String,
    #[schemars(length(min = 1, max = 256))]
    pub display: String,
    #[schemars(range(min = 0.0, max = 100.0))]
    pub opening_width: f64,
}
impl WidthsConfigure {
    pub fn validate(&self) -> Result<(), Failure> {
        if self.connection_id.is_empty()
            || self.connection_id.len() > 256
            || self.display.is_empty()
            || self.display.len() > 256
            || self.connection_id.contains('\0')
            || self.display.contains('\0')
            || !self.opening_width.is_finite()
            || self.opening_width <= 0.0
            || self.opening_width > 100.0
        {
            return Err(Failure::new(
                ErrorCode::Validation,
                "Invalid width observer configuration",
                false,
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WidthsStatus {
    pub available: bool,
    #[schemars(range(min = 0.0, max = 100.0))]
    pub opening_width: f64,
    pub relaxed: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn percentages_and_session_identifiers_fail_closed() {
        let mut params = WidthsConfigure {
            connection_id: "fixture".into(),
            display: ":99".into(),
            opening_width: 50.0,
        };
        assert!(params.validate().is_ok());
        for value in [0.0, -1.0, 100.001, f64::NAN, f64::INFINITY] {
            params.opening_width = value;
            assert!(params.validate().is_err());
        }
        params.opening_width = 100.0;
        assert!(params.validate().is_ok());
        params.display.push('\0');
        assert!(params.validate().is_err());
    }
}
