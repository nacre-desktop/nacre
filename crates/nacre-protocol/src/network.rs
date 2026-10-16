//! Event-source availability, not a network/device snapshot or an action API.
use crate::{ErrorCode, Failure, MAX_SAFE_INTEGER};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct NetworkWatch {
    pub available: bool,
    #[schemars(range(min = 0, max = 9007199254740991_u64))]
    pub generation: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(max = 256))]
    pub error: Option<String>,
}

impl NetworkWatch {
    pub fn validate(&self) -> Result<(), Failure> {
        if self.generation > MAX_SAFE_INTEGER || self.error.as_ref().is_some_and(|e| e.len() > 256)
        {
            return Err(Failure::new(
                ErrorCode::Validation,
                "Invalid network watch state",
                false,
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn trigger_bounds_are_checked_without_rejecting_future_optional_fields() {
        let mut state: NetworkWatch = serde_json::from_value(serde_json::json!({
            "available":true,"generation":3,"futureOptional":"ignored"
        }))
        .unwrap();
        assert!(state.validate().is_ok());
        state.generation = MAX_SAFE_INTEGER + 1;
        assert!(state.validate().is_err());
        state.generation = 3;
        state.error = Some("x".repeat(257));
        assert!(state.validate().is_err());
    }
}
