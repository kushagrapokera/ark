use std::fmt;
use std::io;

#[derive(Debug)]
pub enum UpdateError {
    Io(io::Error),
    InvalidVersion(String),
    UnsupportedTarget(String),
    InvalidManifest(String),
    InsecureUrl(String),
    DownloadTooLarge { size: u64, max: u64 },
    SizeMismatch { expected: u64, actual: u64 },
    ChecksumMismatch,
    SignatureInvalid(String),
    UnsafeArchive(String),
    MissingTool(&'static str),
    CommandFailed { tool: String, detail: String },
    InsufficientDisk { available: u64, required: u64 },
    InstallLayout(String),
    StagedRelease(String),
    RollbackUnavailable,
}

impl fmt::Display for UpdateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::InvalidVersion(v) => write!(f, "invalid version: {v}"),
            Self::UnsupportedTarget(t) => write!(f, "unsupported target: {t}"),
            Self::InvalidManifest(m) => write!(f, "invalid update manifest: {m}"),
            Self::InsecureUrl(u) => write!(f, "insecure update URL: {u}"),
            Self::DownloadTooLarge { size, max } => {
                write!(f, "download is too large: {size} bytes (maximum {max})")
            }
            Self::SizeMismatch { expected, actual } => {
                write!(
                    f,
                    "download size mismatch: expected {expected} bytes, got {actual}"
                )
            }
            Self::ChecksumMismatch => write!(f, "update checksum mismatch"),
            Self::SignatureInvalid(m) => write!(f, "update signature verification failed: {m}"),
            Self::UnsafeArchive(m) => write!(f, "unsafe update archive: {m}"),
            Self::MissingTool(tool) => write!(f, "required tool not found: {tool}"),
            Self::CommandFailed { tool, detail } => write!(f, "{tool} failed: {detail}"),
            Self::InsufficientDisk {
                available,
                required,
            } => {
                write!(
                    f,
                    "insufficient disk space: {available} bytes available, {required} required"
                )
            }
            Self::InstallLayout(m) => write!(f, "invalid install layout: {m}"),
            Self::StagedRelease(m) => write!(f, "invalid staged release: {m}"),
            Self::RollbackUnavailable => write!(f, "no previous ark version is available"),
        }
    }
}

impl std::error::Error for UpdateError {}

impl From<io::Error> for UpdateError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}
