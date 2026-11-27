use std::env;
use std::path::PathBuf;

use super::{UpdateError, Version};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstallLayout {
    data_home: PathBuf,
    bin_home: PathBuf,
    state_home: PathBuf,
    cache_home: PathBuf,
    config_home: PathBuf,
}

impl InstallLayout {
    pub fn new(
        data_home: PathBuf,
        bin_home: PathBuf,
        state_home: PathBuf,
        cache_home: PathBuf,
        config_home: PathBuf,
    ) -> Self {
        Self {
            data_home,
            bin_home,
            state_home,
            cache_home,
            config_home,
        }
    }

    pub fn detect() -> Result<Self, UpdateError> {
        let home = env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| UpdateError::InstallLayout("HOME is not set".into()))?;
        Ok(Self::new(
            env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".local/share")),
            env::var_os("XDG_BIN_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".local/bin")),
            env::var_os("XDG_STATE_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".local/state")),
            env::var_os("XDG_CACHE_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".cache")),
            env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| home.join(".config")),
        ))
    }

    pub fn app_data_dir(&self) -> PathBuf {
        self.ark_data_dir()
    }

    pub fn ark_data_dir(&self) -> PathBuf {
        self.data_home.join("ark")
    }

    pub fn versions_dir(&self) -> PathBuf {
        self.ark_data_dir().join("versions")
    }

    pub fn version_dir(&self, version: &Version) -> PathBuf {
        self.versions_dir().join(version.to_string())
    }

    pub fn current_link(&self) -> PathBuf {
        self.ark_data_dir().join("current")
    }

    pub fn bin_link(&self) -> PathBuf {
        self.bin_home.join("ark")
    }

    pub fn state_dir(&self) -> PathBuf {
        self.state_home.join("ark")
    }

    pub fn updater_state_dir(&self) -> PathBuf {
        self.state_dir().join("updater")
    }

    pub fn install_file(&self) -> PathBuf {
        self.state_dir().join("install.json")
    }

    pub fn staged_file(&self) -> PathBuf {
        self.updater_state_dir().join("staged.json")
    }

    pub fn cache_dir(&self) -> PathBuf {
        self.cache_home.join("ark/updater")
    }

    pub fn config_dir(&self) -> PathBuf {
        self.config_home.join("ark")
    }
}
