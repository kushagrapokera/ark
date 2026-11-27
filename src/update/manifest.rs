use std::collections::BTreeMap;

use super::json::JsonValue;
use super::Target;
use super::{
    required_bool, required_object, required_string, required_u64, validate_url, UpdateError,
    Version, MAX_DOWNLOAD_BYTES,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReleaseAsset {
    pub url: String,
    pub size: u64,
    pub sha256: String,
    pub signature: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReleaseManifest {
    pub version: Version,
    pub channel: String,
    pub minimum_version: Version,
    pub critical: bool,
    pub notes_url: String,
    pub assets: BTreeMap<String, ReleaseAsset>,
}

impl ReleaseManifest {
    pub fn parse(input: &str, allow_http: bool) -> Result<Self, UpdateError> {
        let value = super::json::parse(input).map_err(UpdateError::InvalidManifest)?;
        let root = value
            .as_object()
            .ok_or_else(|| UpdateError::InvalidManifest("root must be an object".into()))?;
        let version = Version::parse(required_string(root, "version")?)?;
        let channel = required_string(root, "channel")?.to_owned();
        if channel != "stable" && channel != "beta" {
            return Err(UpdateError::InvalidManifest(format!(
                "unsupported channel: {channel}"
            )));
        }
        let minimum_version = Version::parse(required_string(root, "minimum_version")?)?;
        let critical = required_bool(root, "critical")?;
        let notes_url = validate_url(required_string(root, "notes_url")?, allow_http)?;
        let published_at = required_string(root, "published_at")?;
        if published_at.is_empty() || published_at.len() > 64 {
            return Err(UpdateError::InvalidManifest("invalid published_at".into()));
        }
        let assets = required_object(root, "assets")?;
        if assets.is_empty() {
            return Err(UpdateError::InvalidManifest(
                "assets must not be empty".into(),
            ));
        }
        let mut parsed_assets = BTreeMap::new();
        for (key, value) in assets {
            let asset = value.as_object().ok_or_else(|| {
                UpdateError::InvalidManifest(format!("asset {key} must be an object"))
            })?;
            let url = validate_url(required_string(asset, "url")?, allow_http)?;
            let size = required_u64(asset, "size")?;
            if size == 0 || size > MAX_DOWNLOAD_BYTES {
                return Err(UpdateError::DownloadTooLarge {
                    size,
                    max: MAX_DOWNLOAD_BYTES,
                });
            }
            let sha256 = required_string(asset, "sha256")?;
            if sha256.len() != 64 || !sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
                return Err(UpdateError::InvalidManifest(format!(
                    "asset {key} has invalid sha256"
                )));
            }
            let signature = match asset.get("signature") {
                None | Some(JsonValue::Null) => None,
                Some(value) => Some(validate_url(
                    value.as_str().ok_or_else(|| {
                        UpdateError::InvalidManifest("signature must be a string".into())
                    })?,
                    allow_http,
                )?),
            };
            if channel == "stable" && signature.is_none() {
                return Err(UpdateError::InvalidManifest(format!(
                    "stable asset {key} is missing a signature"
                )));
            }
            parsed_assets.insert(
                key.clone(),
                ReleaseAsset {
                    url,
                    size,
                    sha256: sha256.to_ascii_lowercase(),
                    signature,
                },
            );
        }
        Ok(Self {
            version,
            channel,
            minimum_version,
            critical,
            notes_url,
            assets: parsed_assets,
        })
    }

    pub fn asset(&self, target: Target) -> Result<&ReleaseAsset, UpdateError> {
        self.assets
            .get(target.asset_key())
            .ok_or_else(|| UpdateError::UnsupportedTarget(target.asset_key().into()))
    }

    pub fn is_newer_than(&self, current: &Version) -> bool {
        self.version > *current
    }
}
