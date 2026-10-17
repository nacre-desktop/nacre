//! Niri actions stay bound to the socket the UI selected, across reconnects.
use crate::{ErrorCode, Failure};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct NiriSource {
    #[schemars(length(min = 1, max = 4096))]
    pub socket: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct NiriConnection {
    #[schemars(length(min = 1, max = 256))]
    pub connection_id: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct NiriAction {
    #[schemars(length(min = 1, max = 256))]
    pub connection_id: String,
    #[schemars(length(min = 2, max = 64))]
    pub argv: Vec<String>,
}
impl NiriSource {
    pub fn validate(&self) -> Result<(), Failure> {
        if !self.socket.starts_with('/') || self.socket.len() > 4096 || self.socket.contains('\0') {
            return Err(invalid());
        }
        Ok(())
    }
}
impl NiriAction {
    pub fn validate(&self) -> Result<(), Failure> {
        if self.connection_id.is_empty()
            || self.connection_id.len() > 256
            || self.connection_id.contains('\0')
            || !(2..=64).contains(&self.argv.len())
            || self.argv.first().is_none_or(|arg| arg != "action")
            || self
                .argv
                .iter()
                .any(|arg| arg.len() > 4096 || arg.contains('\0'))
        {
            return Err(invalid());
        }
        Ok(())
    }
}
fn invalid() -> Failure {
    Failure::new(ErrorCode::Validation, "Invalid compositor request", false).dispatch_state(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn session_and_argument_bounds_fail_closed() {
        for socket in ["relative", "", "/bad\0socket"] {
            assert!(NiriSource {
                socket: socket.into()
            }
            .validate()
            .is_err());
        }
        assert!(NiriSource {
            socket: "/private/niri.sock".into()
        }
        .validate()
        .is_ok());
        let mut request = NiriAction {
            connection_id: "1:2".into(),
            argv: vec!["action".into(), "reload".into()],
        };
        assert!(request.validate().is_ok());
        request.argv[0] = "query".into();
        assert!(request.validate().is_err());
        request.argv[0] = "action".into();
        request.argv.push("x".repeat(4097));
        assert!(request.validate().is_err());
        request.argv.pop();
        request.connection_id.clear();
        assert!(request.validate().is_err());
    }
}
