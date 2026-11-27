use std::cmp::Ordering;
use std::env;
use std::fmt;

use super::UpdateError;

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct Version {
    major: u64,
    minor: u64,
    patch: u64,
    prerelease: Vec<PrereleaseIdentifier>,
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
enum PrereleaseIdentifier {
    Numeric(u64),
    Alpha(String),
}

impl Version {
    pub fn parse(input: &str) -> Result<Self, UpdateError> {
        let invalid = || UpdateError::InvalidVersion(input.to_owned());
        if input.is_empty() || input.len() > 128 || input.chars().any(char::is_whitespace) {
            return Err(invalid());
        }
        let (without_build, build) = match input.split_once('+') {
            Some((version, build)) => (version, Some(build)),
            None => (input, None),
        };
        if let Some(build) = build {
            if !valid_identifiers(build, false) {
                return Err(invalid());
            }
        }
        let (core, prerelease) = match without_build.split_once('-') {
            Some((version, pre)) => (version, Some(pre)),
            None => (without_build, None),
        };
        let mut parts = core.split('.');
        let major = parse_numeric_part(parts.next().ok_or_else(invalid)?)?;
        let minor = parse_numeric_part(parts.next().ok_or_else(invalid)?)?;
        let patch = parse_numeric_part(parts.next().ok_or_else(invalid)?)?;
        if parts.next().is_some() {
            return Err(invalid());
        }
        let prerelease = match prerelease {
            Some(pre) => {
                if !valid_identifiers(pre, true) {
                    return Err(invalid());
                }
                let mut identifiers = Vec::new();
                for part in pre.split('.') {
                    if part.bytes().all(|b| b.is_ascii_digit()) {
                        let value = part.parse::<u64>().map_err(|_| invalid())?;
                        identifiers.push(PrereleaseIdentifier::Numeric(value));
                    } else {
                        identifiers.push(PrereleaseIdentifier::Alpha(part.to_owned()));
                    }
                }
                identifiers
            }
            None => Vec::new(),
        };
        Ok(Self {
            major,
            minor,
            patch,
            prerelease,
        })
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        if !self.prerelease.is_empty() {
            write!(f, "-")?;
            for (index, identifier) in self.prerelease.iter().enumerate() {
                if index > 0 {
                    write!(f, ".")?;
                }
                match identifier {
                    PrereleaseIdentifier::Numeric(value) => write!(f, "{value}")?,
                    PrereleaseIdentifier::Alpha(value) => write!(f, "{value}")?,
                }
            }
        }
        Ok(())
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.major, self.minor, self.patch)
            .cmp(&(other.major, other.minor, other.patch))
            .then_with(
                || match (self.prerelease.is_empty(), other.prerelease.is_empty()) {
                    (true, false) => Ordering::Greater,
                    (false, true) => Ordering::Less,
                    _ => self.prerelease.cmp(&other.prerelease),
                },
            )
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PrereleaseIdentifier {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (Self::Numeric(a), Self::Numeric(b)) => a.cmp(b),
            (Self::Numeric(_), Self::Alpha(_)) => Ordering::Less,
            (Self::Alpha(_), Self::Numeric(_)) => Ordering::Greater,
            (Self::Alpha(a), Self::Alpha(b)) => a.cmp(b),
        }
    }
}

impl PartialOrd for PrereleaseIdentifier {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn parse_numeric_part(part: &str) -> Result<u64, UpdateError> {
    if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
        return Err(UpdateError::InvalidVersion(part.to_owned()));
    }
    if part.len() > 1 && part.starts_with('0') {
        return Err(UpdateError::InvalidVersion(part.to_owned()));
    }
    part.parse()
        .map_err(|_| UpdateError::InvalidVersion(part.to_owned()))
}

fn valid_identifiers(input: &str, reject_numeric_leading_zero: bool) -> bool {
    !input.is_empty()
        && input.split('.').all(|part| {
            !part.is_empty()
                && part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
                && !(reject_numeric_leading_zero
                    && part.len() > 1
                    && part.starts_with('0')
                    && part.bytes().all(|b| b.is_ascii_digit()))
        })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Target {
    X86_64,
    Aarch64,
}

impl Target {
    pub fn from_arch(arch: &str) -> Result<Self, UpdateError> {
        match arch {
            "x86_64" => Ok(Self::X86_64),
            "aarch64" | "arm64" => Ok(Self::Aarch64),
            other => Err(UpdateError::UnsupportedTarget(other.to_owned())),
        }
    }

    pub fn detect() -> Result<Self, UpdateError> {
        match env::var("ARK_ARCH") {
            Ok(arch) => Self::from_arch(&arch),
            Err(_) => Self::from_arch(env::consts::ARCH),
        }
    }

    pub fn asset_key(self) -> &'static str {
        match self {
            Self::X86_64 => "x86_64-unknown-linux-gnu",
            Self::Aarch64 => "aarch64-unknown-linux-gnu",
        }
    }
}
