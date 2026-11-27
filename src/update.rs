//! Shared, dependency-light models for ark's release updater.

mod archive;
mod error;
mod install;
mod json;
mod layout;
mod manifest;
mod state;
mod version;

pub use archive::validate_archive_listing;
pub use error::UpdateError;
pub use install::{InstallInfo, StagedRelease};
pub use layout::InstallLayout;
pub use manifest::{ReleaseAsset, ReleaseManifest};
pub use state::UpdateState;
pub use version::{Target, Version};

use std::collections::BTreeMap;

use json::JsonValue;

pub const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
pub const MAX_DOWNLOAD_BYTES: u64 = 128 * 1024 * 1024;

pub fn json_string(value: &str) -> String {
    let mut output = String::with_capacity(value.len() + 2);
    output.push('"');
    for ch in value.chars() {
        match ch {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            ch if ch < '\u{0020}' => output.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => output.push(ch),
        }
    }
    output.push('"');
    output
}

fn required_object<'a>(
    object: &'a BTreeMap<String, JsonValue>,
    key: &str,
) -> Result<&'a BTreeMap<String, JsonValue>, UpdateError> {
    object
        .get(key)
        .and_then(JsonValue::as_object)
        .ok_or_else(|| UpdateError::InvalidManifest(format!("missing object field: {key}")))
}

fn required_string<'a>(
    object: &'a BTreeMap<String, JsonValue>,
    key: &str,
) -> Result<&'a str, UpdateError> {
    object
        .get(key)
        .and_then(JsonValue::as_str)
        .ok_or_else(|| UpdateError::InvalidManifest(format!("missing string field: {key}")))
}

fn required_bool(object: &BTreeMap<String, JsonValue>, key: &str) -> Result<bool, UpdateError> {
    object
        .get(key)
        .and_then(JsonValue::as_bool)
        .ok_or_else(|| UpdateError::InvalidManifest(format!("missing bool field: {key}")))
}

fn required_u64(object: &BTreeMap<String, JsonValue>, key: &str) -> Result<u64, UpdateError> {
    object
        .get(key)
        .and_then(JsonValue::as_u64)
        .ok_or_else(|| UpdateError::InvalidManifest(format!("missing integer field: {key}")))
}

fn validate_url(url: &str, allow_http: bool) -> Result<String, UpdateError> {
    if url.len() > 2048
        || url.chars().any(char::is_control)
        || url.contains('"')
        || url.contains('\\')
    {
        return Err(UpdateError::InvalidManifest("invalid URL".into()));
    }
    if url.starts_with("https://") && url.len() > "https://".len() {
        return Ok(url.to_owned());
    }
    if allow_http && url.starts_with("http://") && url.len() > "http://".len() {
        return Ok(url.to_owned());
    }
    Err(UpdateError::InsecureUrl(url.to_owned()))
}

#[cfg(test)]
mod tests;
