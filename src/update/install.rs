use std::path::PathBuf;

use super::{json_string, required_string, validate_url, UpdateError, Version};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstallInfo {
    pub version: Version,
    pub arch: String,
    pub channel: String,
    pub manifest_url: String,
    pub installed_at: String,
}

impl InstallInfo {
    pub fn parse(input: &str) -> Result<Self, UpdateError> {
        let value = super::json::parse(input).map_err(UpdateError::InvalidManifest)?;
        let object = value.as_object().ok_or_else(|| {
            UpdateError::InstallLayout("install.json root must be an object".into())
        })?;
        Ok(Self {
            version: Version::parse(required_string(object, "version")?)?,
            arch: required_string(object, "arch")?.to_owned(),
            channel: required_string(object, "channel")?.to_owned(),
            manifest_url: validate_url(required_string(object, "manifest_url")?, true)?,
            installed_at: required_string(object, "installed_at")?.to_owned(),
        })
    }

    pub fn to_json(&self) -> String {
        format!(
            "{{\n  \"version\": {},\n  \"arch\": {},\n  \"channel\": {},\n  \"manifest_url\": {},\n  \"installed_at\": {}\n}}\n",
            json_string(&self.version.to_string()),
            json_string(&self.arch),
            json_string(&self.channel),
            json_string(&self.manifest_url),
            json_string(&self.installed_at),
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StagedRelease {
    pub version: Version,
    pub path: PathBuf,
    pub channel: String,
    pub manifest_url: String,
}

impl StagedRelease {
    pub fn parse(input: &str) -> Result<Self, UpdateError> {
        let value = super::json::parse(input).map_err(UpdateError::StagedRelease)?;
        let object = value.as_object().ok_or_else(|| {
            UpdateError::StagedRelease("staged.json root must be an object".into())
        })?;
        Ok(Self {
            version: Version::parse(required_string(object, "version")?)?,
            path: PathBuf::from(required_string(object, "path")?),
            channel: required_string(object, "channel")?.to_owned(),
            manifest_url: validate_url(required_string(object, "manifest_url")?, true)?,
        })
    }

    pub fn to_json(&self) -> String {
        format!(
            "{{\n  \"version\": {},\n  \"path\": {},\n  \"channel\": {},\n  \"manifest_url\": {}\n}}\n",
            json_string(&self.version.to_string()),
            json_string(&self.path.to_string_lossy()),
            json_string(&self.channel),
            json_string(&self.manifest_url),
        )
    }
}
