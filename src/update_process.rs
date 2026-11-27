use std::path::PathBuf;
use std::time::SystemTime;

use ark::update::{InstallLayout, UpdateState, Version};
use winit::event_loop::EventLoopProxy;

use crate::update_protocol::UpdateEvent;
use crate::watch::Wake;

mod rate;
mod runner;
#[cfg(test)]
mod tests;

#[cfg(test)]
use rate::AUTO_CHECK_INTERVAL;
pub use rate::{automatic_check_due, record_successful_check};
use runner::spawn_helper;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateCommand {
    Check,
    Stage,
    Apply,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateAction {
    Check,
    Install,
    Later,
    Restart,
    TogglePanel,
}

#[derive(Clone, Debug, Default)]
pub struct UpdateUi {
    pub state: UpdateState,
    pub open: bool,
    pub version: Option<Version>,
    pub size: Option<u64>,
    pub notes_url: Option<String>,
    pub percent: u8,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub status: Option<String>,
}

impl UpdateUi {
    pub fn apply_event(&mut self, event: UpdateEvent, automatic: bool) {
        self.error_code = None;
        self.error_message = None;
        match event {
            UpdateEvent::CheckStarted => {
                self.state = UpdateState::Checking;
                self.open = !automatic;
                self.status = None;
                self.percent = 0;
            }
            UpdateEvent::UpdateAvailable {
                version,
                size,
                notes_url,
            } => {
                self.state = UpdateState::Available;
                self.open = true;
                self.version = Some(version);
                self.size = Some(size);
                self.notes_url = Some(notes_url);
                self.status = None;
            }
            UpdateEvent::UpToDate { version } => {
                self.state = UpdateState::Idle;
                self.open = !automatic;
                self.version = Some(version);
                self.size = None;
                self.notes_url = None;
                self.percent = 100;
                self.status = (!automatic).then(|| "ark is up to date".to_owned());
            }
            UpdateEvent::DownloadStarted { version } => {
                self.state = UpdateState::Downloading;
                self.open = true;
                self.version = Some(version);
                self.percent = 0;
                self.status = None;
            }
            UpdateEvent::Progress(percent) => {
                self.state = UpdateState::Downloading;
                self.open = true;
                self.percent = self.percent.max(percent);
                self.status = None;
            }
            UpdateEvent::Staged { version } => {
                self.state = UpdateState::Staged;
                self.open = true;
                self.version = Some(version);
                self.percent = 100;
                self.status = None;
            }
            UpdateEvent::Ready => {
                self.state = UpdateState::Staged;
                self.open = true;
                self.percent = 100;
                self.status = None;
            }
            UpdateEvent::Installed { version } => {
                self.state = UpdateState::Installed;
                self.open = true;
                self.version = Some(version);
                self.percent = 100;
                self.status = None;
            }
            UpdateEvent::Error { code, message } => {
                self.state = UpdateState::Failed;
                self.open = !automatic;
                self.error_code = Some(code);
                self.error_message = Some(message);
                self.status = None;
            }
        }
    }

    pub fn fail(&mut self, code: &str, message: impl Into<String>, automatic: bool) {
        self.apply_event(
            UpdateEvent::Error {
                code: code.to_owned(),
                message: message.into(),
            },
            automatic,
        );
    }

    pub fn later(&mut self) {
        self.open = false;
    }
}

pub struct UpdateController {
    pub ui: UpdateUi,
    layout: Option<InstallLayout>,
    helper: Option<PathBuf>,
    unsupported: Option<String>,
    inflight: Option<UpdateJob>,
    generation: u64,
}

struct UpdateJob {
    generation: u64,
    command: UpdateCommand,
    automatic: bool,
    saw_error: bool,
}

impl UpdateController {
    pub fn detect() -> Self {
        let layout = match InstallLayout::detect() {
            Ok(layout) => Some(layout),
            Err(error) => {
                log::warn!("updates disabled: {error}");
                None
            }
        };
        let helper = std::env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(|parent| parent.join("ark-updater")))
            .filter(is_executable);
        let mut controller = Self::new(layout, helper);
        if controller.supported() {
            let installed = controller
                .layout
                .as_ref()
                .is_some_and(|layout| layout.install_file().is_file());
            if !installed {
                controller.unsupported =
                    Some("ark is not running from a versioned installation".into());
                controller.ui.state = UpdateState::Unsupported;
            }
        }
        controller
    }

    pub fn new(layout: Option<InstallLayout>, helper: Option<PathBuf>) -> Self {
        let unsupported = match (&layout, &helper) {
            (Some(_), Some(_)) => None,
            _ => Some("updates require a versioned ark installation".to_owned()),
        };
        let mut ui = UpdateUi::default();
        if unsupported.is_some() {
            ui.state = UpdateState::Unsupported;
        }
        Self {
            ui,
            layout,
            helper,
            unsupported,
            inflight: None,
            generation: 0,
        }
    }

    pub fn supported(&self) -> bool {
        self.unsupported.is_none()
    }

    pub fn unsupported_reason(&self) -> Option<&str> {
        self.unsupported.as_deref()
    }

    pub fn automatic_due(&self, now: SystemTime) -> bool {
        self.layout
            .as_ref()
            .is_some_and(|layout| automatic_check_due(layout, now))
    }

    pub fn is_busy(&self) -> bool {
        self.inflight.is_some()
    }

    pub fn start(
        &mut self,
        command: UpdateCommand,
        automatic: bool,
        proxy: EventLoopProxy<Wake>,
    ) -> Result<(), String> {
        if self.is_busy() {
            return Err("an update operation is already in progress".into());
        }
        if !self.supported() {
            return Err(self
                .unsupported_reason()
                .unwrap_or("updates are unavailable")
                .to_owned());
        }
        let version = match command {
            UpdateCommand::Apply => Some(
                self.ui
                    .version
                    .as_ref()
                    .ok_or_else(|| "no staged update version is available".to_owned())?
                    .to_string(),
            ),
            UpdateCommand::Check | UpdateCommand::Stage => None,
        };
        self.generation = self.generation.saturating_add(1);
        let generation = self.generation;
        match command {
            UpdateCommand::Check => {
                self.ui.apply_event(UpdateEvent::CheckStarted, automatic);
            }
            UpdateCommand::Stage => {
                self.ui.state = UpdateState::Downloading;
                self.ui.open = true;
                self.ui.percent = 0;
                self.ui.status = None;
                self.ui.error_code = None;
                self.ui.error_message = None;
            }
            UpdateCommand::Apply => {
                self.ui.state = UpdateState::Staged;
                self.ui.open = true;
                self.ui.percent = 100;
                self.ui.status = Some("Installing update…".into());
                self.ui.error_code = None;
                self.ui.error_message = None;
            }
        }
        let helper = self
            .helper
            .as_ref()
            .ok_or_else(|| "update helper is unavailable".to_owned())?;
        if let Err(error) = spawn_helper(helper, command, version.as_deref(), generation, proxy) {
            self.ui.fail("helper-failed", error.clone(), automatic);
            return Err(error);
        }
        self.inflight = Some(UpdateJob {
            generation,
            command,
            automatic,
            saw_error: false,
        });
        Ok(())
    }

    pub fn handle_event(&mut self, generation: u64, event: UpdateEvent) -> bool {
        let Some(job) = self.inflight.as_mut() else {
            return false;
        };
        if job.generation != generation {
            return false;
        }
        if matches!(event, UpdateEvent::Error { .. }) {
            job.saw_error = true;
        }
        self.ui.apply_event(event, job.automatic);
        true
    }

    pub fn finish(
        &mut self,
        generation: u64,
        command: UpdateCommand,
        success: bool,
        error: Option<String>,
    ) -> bool {
        let Some(job) = self.inflight.take() else {
            return false;
        };
        if job.generation != generation || job.command != command {
            self.inflight = Some(job);
            return false;
        }
        if command == UpdateCommand::Check && success && !job.saw_error {
            if let Some(layout) = &self.layout {
                if let Err(error) = record_successful_check(layout, SystemTime::now()) {
                    log::warn!("could not record successful update check: {error}");
                }
            }
        }
        if !success && !job.saw_error {
            self.ui.fail(
                "helper-failed",
                error.unwrap_or_else(|| "update helper failed".into()),
                job.automatic,
            );
        }
        true
    }

    pub fn primary_action(&self) -> Option<UpdateAction> {
        match self.ui.state {
            UpdateState::Idle | UpdateState::Failed => Some(UpdateAction::Check),
            UpdateState::Available | UpdateState::Staged => Some(UpdateAction::Install),
            UpdateState::Installed => Some(UpdateAction::Restart),
            _ => None,
        }
    }

    pub fn title_action(&self) -> Option<UpdateAction> {
        if !self.supported() {
            return None;
        }
        if self.ui.open || self.ui.state != UpdateState::Idle {
            Some(UpdateAction::TogglePanel)
        } else {
            Some(UpdateAction::Check)
        }
    }

    pub fn install_command(&self) -> Option<UpdateCommand> {
        match self.ui.state {
            UpdateState::Available => Some(UpdateCommand::Stage),
            UpdateState::Staged => Some(UpdateCommand::Apply),
            _ => None,
        }
    }

    pub fn restart_target(&self) -> Result<PathBuf, String> {
        let layout = self
            .layout
            .as_ref()
            .ok_or_else(|| "updates require a versioned ark installation".to_owned())?;
        let target = layout.current_link().join("ark");
        if !is_executable(&target) {
            return Err(format!(
                "installed ark executable is unavailable: {}",
                target.display()
            ));
        }
        Ok(target)
    }
}

fn is_executable(path: &PathBuf) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}
