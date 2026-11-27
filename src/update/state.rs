#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UpdateState {
    #[default]
    Idle,
    Checking,
    Available,
    Downloading,
    Verifying,
    Staged,
    Installed,
    Failed,
    Unsupported,
}
