//! The old six lanes share the daemon's bounded provider pool and preference writer.
use super::{
    hub::{failure, Shared},
    menu,
};
use crate::{service, Result};
use nacre_protocol::{
    helpers::{HelperCall, HelperReply, HelperResult, HelperTransfer, HelperTransferRequest},
    Capability, ErrorCode, Failure, Method,
};
use serde_json::{json, Value};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::PathBuf,
    rc::Rc,
};

const INLINE: usize = 384 * 1024;
// A JSON string needs at most six bytes per input byte, plus a small envelope.
// Use the exact legacy encoder only when this conservative bound cannot decide.
fn encoded_bound(output: &str) -> usize {
    output.len().saturating_mul(6).saturating_add(128)
}
fn oversized(id: u64, result: &HelperResult) -> Result<bool> {
    let limit = service::Limits::default().response;
    if encoded_bound(&result.output) <= limit {
        return Ok(false);
    }
    Ok(
        crate::json::encode(&json!({"id":id,"code":result.code,"output":result.output}))?.len()
            > limit,
    )
}

#[derive(Clone, Default)]
pub struct Transfer(Rc<RefCell<Option<tempfile::TempDir>>>);
impl Transfer {
    fn directory(&self) -> Result<PathBuf> {
        if self.0.borrow().is_none() {
            let endpoint = super::socket::path()?;
            super::socket::validate_endpoint(&endpoint)?;
            let directory = tempfile::Builder::new()
                .prefix(&transfer_prefix(&endpoint))
                .permissions(fs::Permissions::from_mode(0o700))
                .tempdir_in(endpoint.parent().ok_or("Missing socket parent")?)?;
            *self.0.borrow_mut() = Some(directory);
        }
        Ok(self
            .0
            .borrow()
            .as_ref()
            .ok_or("Missing transfer lease")?
            .path()
            .into())
    }
    fn open(&self, name: &str) -> Result<File> {
        self.open_bound(name, service::Limits::default().request)
    }
    fn open_bound(&self, name: &str, limit: u64) -> Result<File> {
        if !name.starts_with("packet-")
            || name.len() > 96
            || !name
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
        {
            return Err("Invalid helper packet name".into());
        }
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(nix::libc::O_NOFOLLOW)
            .open(self.directory()?.join(name))?;
        let metadata = file.metadata()?;
        if !metadata.is_file()
            || metadata.uid() != nix::unistd::getuid().as_raw()
            || metadata.mode() & 0o777 != 0o600
            || metadata.nlink() != 1
            || metadata.len() > limit
        {
            return Err("Unsafe or oversized helper packet".into());
        }
        Ok(file)
    }
    fn transfer(&self, params: HelperTransferRequest) -> Result<HelperTransfer> {
        if params.release.len() > 128 {
            return Err("Helper transfer release limit reached".into());
        }
        let directory = self.directory()?;
        for name in params.release {
            // The same ownership/link/name guards apply to deletion. A response
            // may be larger than an input packet, up to the existing limit.
            match self.open_bound(&name, service::Limits::default().response as u64) {
                Ok(_) => fs::remove_file(directory.join(name))?,
                Err(error)
                    if error
                        .downcast_ref::<std::io::Error>()
                        .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound) => {}
                Err(error) => return Err(error),
            }
        }
        let file = if params.reserve == Some(true) {
            if fs::read_dir(&directory)?.take(129).count() >= 128 {
                return Err("Helper transfer queue is full".into());
            }
            let file = tempfile::Builder::new()
                .prefix("packet-")
                .tempfile_in(&directory)?;
            let (_, path) = file.keep()?;
            Some(
                path.file_name()
                    .and_then(|name| name.to_str())
                    .ok_or("Invalid packet name")?
                    .into(),
            )
        } else {
            None
        };
        Ok(HelperTransfer {
            directory: directory.to_str().ok_or("Invalid packet directory")?.into(),
            file,
        })
    }
    fn request(&self, name: &str) -> Result<service::Request> {
        let mut bytes = Vec::new();
        self.open(name)?
            .take(service::Limits::default().request + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > service::Limits::default().request {
            return Err("Helper request is too large".into());
        }
        service::validate(&serde_json::from_slice(&bytes)?)
    }
    fn reply(&self, result: HelperResult) -> Result<HelperReply> {
        if encoded_bound(&result.output) <= INLINE {
            return Ok(HelperReply::Inline(result));
        }
        let bytes = serde_json::to_vec(&result)?;
        if bytes.len() <= INLINE {
            return Ok(HelperReply::Inline(result));
        }
        let directory = self.directory()?;
        if fs::read_dir(&directory)?.take(129).count() >= 128 {
            return Err("Helper transfer queue is full".into());
        }
        let mut file = tempfile::Builder::new()
            .prefix("packet-")
            .tempfile_in(directory)?;
        file.write_all(&bytes)?;
        file.flush()?;
        let (_, path) = file.keep()?;
        Ok(HelperReply::File {
            file: path
                .file_name()
                .and_then(|s| s.to_str())
                .ok_or("Invalid packet name")?
                .into(),
        })
    }
}
pub fn methods() -> BTreeMap<String, Method> {
    [
        (
            "helper.transfer",
            serde_json::from_str(include_str!(
                "../../../../protocol/schema/helper-transfer-request.json"
            ))
            .expect("committed transfer schema"),
            serde_json::from_str(include_str!(
                "../../../../protocol/schema/helper-transfer.json"
            ))
            .expect("committed transfer schema"),
        ),
        (
            "helper.execute",
            serde_json::from_str(include_str!("../../../../protocol/schema/helper-call.json"))
                .expect("committed helper schema"),
            serde_json::from_str(include_str!(
                "../../../../protocol/schema/helper-reply.json"
            ))
            .expect("committed helper schema"),
        ),
    ]
    .into_iter()
    .map(|(name, params, result)| {
        (
            name.into(),
            Method {
                v: 1,
                class: Capability::Desktop,
                timeout: 120000,
                params,
                result,
            },
        )
    })
    .collect()
}
pub async fn dispatch(
    hub: Shared,
    transfer: Transfer,
    method: &str,
    params: Value,
) -> std::result::Result<Value, Failure> {
    if method == "helper.transfer" {
        let params: HelperTransferRequest = serde_json::from_value(params)
            .map_err(|_| failure(ErrorCode::Validation, "Invalid transfer parameters", false))?;
        return transfer
            .transfer(params)
            .and_then(|reply| Ok(serde_json::to_value(reply)?))
            .map_err(|_| {
                failure(
                    ErrorCode::Unavailable,
                    "Cannot allocate helper transfer lease",
                    true,
                )
            });
    }
    let call: HelperCall = serde_json::from_value(params)
        .map_err(|_| failure(ErrorCode::Validation, "Invalid helper call", false))?;
    let request = match call {
        HelperCall::Inline { request } => service::validate(
            &serde_json::to_value(request)
                .map_err(|_| failure(ErrorCode::Validation, "Invalid helper request", false))?,
        ),
        HelperCall::File { file } => transfer.request(&file),
    }
    .map_err(|_| {
        failure(
            ErrorCode::Validation,
            "Invalid helper request or transfer file",
            false,
        )
    })?;
    let index = service::TOOLS
        .iter()
        .position(|v| *v == request.tool)
        .ok_or_else(|| failure(ErrorCode::Validation, "Unsupported helper", false))?;
    let (lane, pool) = {
        let hub = hub.borrow();
        (hub.helper_lanes[index].clone(), hub.providers.clone())
    };
    let _lane = lane.lock().await;
    let mut result = if request.tool == "launcher-preferences" {
        let supported = request.arguments.len() == 1
            && ((request.arguments[0] == "load" && request.input.is_none())
                || (["favorite", "hide", "record"].contains(&request.arguments[0].as_str())
                    && request.input.as_ref().is_some_and(Value::is_object)));
        if !supported {
            HelperResult {
                code: 1,
                output: service::error_output("Unsupported helper operation"),
            }
        } else {
            match menu::dispatch(
                hub,
                "helper.preferences",
                json!({"action":request.arguments[0],"value":request.input}),
                Some(120000),
            )
            .await
            {
                Ok(value) => HelperResult {
                    code: 0,
                    output: format!(
                        "{}\n",
                        crate::json::encode(&value).map_err(|_| failure(
                            ErrorCode::Internal,
                            "Cannot encode preferences",
                            false
                        ))?
                    ),
                },
                Err(error) => HelperResult {
                    code: 1,
                    output: service::error_output(&error.message),
                },
            }
        }
    } else {
        serde_json::from_value(pool.hosted(&request).await.map_err(|mut error| {
            if error.code != ErrorCode::Busy {
                error.retry = false;
                error = error.dispatch_state(true);
            }
            error
        })?)
        .map_err(|_| failure(ErrorCode::Internal, "Invalid helper worker response", false))?
    };
    if oversized(request.id, &result)
        .map_err(|_| failure(ErrorCode::Internal, "Cannot encode helper result", false))?
    {
        result = HelperResult {
            code: 1,
            output: service::error_output("Helper response is too large"),
        };
    }
    transfer
        .reply(result)
        .and_then(|reply| Ok(serde_json::to_value(reply)?))
        .map_err(|_| {
            failure(
                ErrorCode::Unavailable,
                "Cannot publish helper response",
                false,
            )
            .dispatch_state(true)
        })
}

fn transfer_prefix(endpoint: &std::path::Path) -> String {
    use sha2::{Digest, Sha256};
    use std::os::unix::ffi::OsStrExt;
    format!(
        "helper-packets-{:x}-",
        Sha256::digest(endpoint.as_os_str().as_bytes())
    )
}
/// Called only after owning this endpoint's exclusive daemon lock. Other
/// endpoints in the same directory have a different prefix and are untouched.
pub fn cleanup() -> Result<()> {
    let endpoint = super::socket::path()?;
    super::socket::validate_endpoint(&endpoint)?;
    cleanup_at(
        endpoint.parent().ok_or("Missing socket parent")?,
        &transfer_prefix(&endpoint),
    )
}
fn cleanup_at(parent: &std::path::Path, prefix: &str) -> Result<()> {
    for entry in fs::read_dir(parent)? {
        let entry = entry?;
        if !entry.file_name().to_string_lossy().starts_with(prefix) {
            continue;
        }
        let metadata = fs::symlink_metadata(entry.path())?;
        if !metadata.is_dir()
            || metadata.file_type().is_symlink()
            || metadata.uid() != nix::unistd::getuid().as_raw()
            || metadata.mode() & 0o777 != 0o700
        {
            return Err("Unsafe stale helper transfer directory".into());
        }
        for file in fs::read_dir(entry.path())? {
            let file = file?;
            let metadata = fs::symlink_metadata(file.path())?;
            if !file.file_name().to_string_lossy().starts_with("packet-")
                || !metadata.is_file()
                || metadata.file_type().is_symlink()
                || metadata.uid() != nix::unistd::getuid().as_raw()
                || metadata.mode() & 0o777 != 0o600
                || metadata.nlink() != 1
            {
                return Err("Unsafe stale helper transfer packet".into());
            }
        }
        fs::remove_dir_all(entry.path())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};
    #[test]
    fn legacy_response_limit_keeps_exact_boundary_and_unicode_escaping() -> Result<()> {
        let id = 9_007_199_254_740_991;
        let limit = service::Limits::default().response;
        let envelope = crate::json::encode(&json!({"id":id,"code":0,"output":""}))?.len();
        assert!(!oversized(
            id,
            &HelperResult {
                code: 0,
                output: "x".repeat(limit - envelope)
            }
        )?);
        assert!(oversized(
            id,
            &HelperResult {
                code: 0,
                output: "x".repeat(limit - envelope + 1)
            }
        )?);
        assert!(oversized(
            id,
            &HelperResult {
                code: 0,
                output: "\u{7f}".repeat(limit / 6 + 1)
            }
        )?);
        assert!(oversized(
            id,
            &HelperResult {
                code: 0,
                output: "😀".repeat(limit / 12 + 1)
            }
        )?);
        Ok(())
    }
    fn lease() -> Result<Transfer> {
        Ok(Transfer(Rc::new(RefCell::new(Some(tempfile::tempdir()?)))))
    }
    #[test]
    fn qml_packets_are_reserved_private_bounded_and_released_without_weakening_guards() -> Result<()>
    {
        let transfer = lease()?;
        let reply = transfer.transfer(HelperTransferRequest {
            reserve: Some(true),
            release: vec![],
        })?;
        let file = reply.file.ok_or("Missing reserved packet")?;
        let path = PathBuf::from(&reply.directory).join(&file);
        assert_eq!(fs::metadata(&path)?.mode() & 0o777, 0o600);
        fs::write(&path, b"{}")?;
        assert!(transfer.open(&file).is_ok());
        assert_eq!(fs::metadata(&path)?.mode() & 0o777, 0o600);
        transfer.transfer(HelperTransferRequest {
            reserve: None,
            release: vec![file.clone()],
        })?;
        assert!(!path.exists());
        transfer.transfer(HelperTransferRequest {
            reserve: None,
            release: vec![file],
        })?;
        assert!(transfer
            .transfer(HelperTransferRequest {
                reserve: None,
                release: vec!["../outside".into()]
            })
            .is_err());
        let outside = tempfile::NamedTempFile::new()?;
        let directory = transfer.directory()?;
        symlink(outside.path(), directory.join("packet-link"))?;
        assert!(transfer
            .transfer(HelperTransferRequest {
                reserve: None,
                release: vec!["packet-link".into()]
            })
            .is_err());
        assert!(outside.path().exists());
        fs::remove_file(directory.join("packet-link"))?;
        for _ in 0..128 {
            transfer.transfer(HelperTransferRequest {
                reserve: Some(true),
                release: vec![],
            })?;
        }
        assert!(transfer
            .transfer(HelperTransferRequest {
                reserve: Some(true),
                release: vec![]
            })
            .is_err());
        Ok(())
    }
    #[test]
    fn file_packets_retain_large_legacy_input_and_cleanup_the_lease() -> Result<()> {
        let transfer = lease()?;
        let directory = transfer.directory()?;
        let payload = json!({"id":1,"tool":"notification-history","arguments":["save"],"input":["x".repeat(2*1024*1024)]});
        let path = directory.join("packet-large");
        fs::write(&path, serde_json::to_vec(&payload)?)?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
        assert_eq!(
            serde_json::to_value(transfer.request("packet-large")?)?,
            payload
        );
        let result = HelperResult {
            code: 0,
            output: "x".repeat(2 * 1024 * 1024),
        };
        let HelperReply::File { file } = transfer.reply(result.clone())? else {
            panic!("Large output must use a file");
        };
        assert_eq!(
            serde_json::from_slice::<HelperResult>(&fs::read(directory.join(file))?)?,
            result
        );
        drop(transfer);
        assert!(!directory.exists());
        Ok(())
    }
    #[test]
    fn linked_hardlinked_public_and_out_of_scope_packets_are_rejected() -> Result<()> {
        let transfer = lease()?;
        let outside = tempfile::tempdir()?;
        let target = outside.path().join("untouched");
        fs::write(&target, b"private sentinel")?;
        fs::set_permissions(&target, fs::Permissions::from_mode(0o600))?;
        let directory = transfer.directory()?;
        symlink(&target, directory.join("packet-link"))?;
        assert!(transfer.request("packet-link").is_err());
        fs::hard_link(&target, directory.join("packet-hardlink"))?;
        assert!(transfer.request("packet-hardlink").is_err());
        for name in [
            "../untouched",
            "/tmp/packet-other",
            "packet-../other",
            "packet-a\0",
        ] {
            assert!(transfer.request(name).is_err());
        }
        let public = directory.join("packet-public");
        fs::write(&public, b"{}")?;
        fs::set_permissions(&public, fs::Permissions::from_mode(0o644))?;
        assert!(transfer.request("packet-public").is_err());
        assert_eq!(fs::read(&target)?, b"private sentinel");
        Ok(())
    }
    #[test]
    fn inline_and_file_payloads_are_exclusive_and_optional_extensions_are_ignored() {
        let request = json!({"id":1,"tool":"network-state","arguments":[],"input":null});
        assert!(
            serde_json::from_value::<HelperCall>(json!({"request":request,"file":"packet-a"}))
                .is_err()
        );
        assert!(
            serde_json::from_value::<HelperCall>(json!({"file":"packet-a","extra":true})).is_ok()
        );
    }
    #[test]
    fn restart_cleanup_is_endpoint_scoped_and_refuses_links() -> Result<()> {
        let root = tempfile::tempdir()?;
        let prefix = transfer_prefix(std::path::Path::new("/private/daemon.sock"));
        let own = root.path().join(format!("{prefix}stale"));
        fs::create_dir(&own)?;
        fs::set_permissions(&own, fs::Permissions::from_mode(0o700))?;
        let packet = own.join("packet-old");
        fs::write(&packet, b"fixture")?;
        fs::set_permissions(&packet, fs::Permissions::from_mode(0o600))?;
        let other = root.path().join("helper-packets-other-endpoint");
        fs::create_dir(&other)?;
        cleanup_at(root.path(), &prefix)?;
        assert!(!own.exists());
        assert!(other.exists());
        symlink(&other, &own)?;
        assert!(cleanup_at(root.path(), &prefix).is_err());
        assert!(other.exists());
        Ok(())
    }
}
